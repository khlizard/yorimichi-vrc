//! Shared state and the long-running operations (import, vacuum, export, timer, window helpers).

use std::collections::HashSet;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use chrono::Local;
use rusqlite::Connection;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_dialog::DialogExt;
use vrclogger_core::data::db;
use vrclogger_core::data::import::ImportService;
use vrclogger_core::export;
use vrclogger_core::parsing::ParseOptions;

use crate::maintenance::should_vacuum;
use crate::photos::{self, PhotosChanged};
use crate::settings::{settings_dir, Settings};

const PRUNE_INTERVAL: Duration = Duration::from_secs(3600);

pub struct AppState {
    pub settings: Mutex<Settings>,
    /// Connection used by the query commands (None if the DB could not be opened).
    pub query: Mutex<Option<Connection>>,
    pub importing: AtomicBool,
    pub next_prune_at: Mutex<Option<Instant>>,
    pub exit_requested: AtomicBool,
    pub status: Mutex<String>,
    pub timer_generation: AtomicU64,
    /// Downloaded Velopack update waiting for the user to restart.
    pub pending_update: Mutex<Option<velopack::UpdateInfo>>,
    pub next_photo_scan_at: Mutex<Option<Instant>>,
    /// Bounds concurrent thumbnail generation.
    pub thumb_gate: tokio::sync::Semaphore,
    /// Thumbnail keys that failed to generate this session (not retried).
    pub thumb_failed: Mutex<HashSet<String>>,
    pub thumb_generation: AtomicU64,
}

impl AppState {
    pub fn new(settings: Settings) -> Self {
        Self {
            settings: Mutex::new(settings),
            query: Mutex::new(None),
            importing: AtomicBool::new(false),
            next_prune_at: Mutex::new(None),
            exit_requested: AtomicBool::new(false),
            status: Mutex::new(String::new()),
            timer_generation: AtomicU64::new(0),
            pending_update: Mutex::new(None),
            next_photo_scan_at: Mutex::new(None),
            thumb_gate: tokio::sync::Semaphore::new(3),
            thumb_failed: Mutex::new(HashSet::new()),
            thumb_generation: AtomicU64::new(0),
        }
    }
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct StatusDto {
    pub status: String,
    pub importing: bool,
    pub db_path: String,
    pub db_size_bytes: i64,
    pub reclaimable_bytes: i64,
    pub vacuum_recommended: bool,
    pub version: String,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ImportFinished {
    changed: bool,
    automatic: bool,
}

/// Opens (creating the parent directory and schema if needed) a database connection.
pub fn open_db(path: &Path) -> vrclogger_core::Result<Connection> {
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let conn = db::open(path)?;
    db::ensure_created(&conn)?;
    Ok(conn)
}

pub fn set_status(app: &AppHandle, msg: impl Into<String>) {
    let msg = msg.into();
    *app.state::<AppState>().status.lock().unwrap() = msg.clone();
    let _ = app.emit("status", msg);
}

fn set_importing(app: &AppHandle, value: bool) {
    app.state::<AppState>().importing.store(value, Ordering::SeqCst);
    let _ = app.emit("importing", value);
}

pub fn release_importing(app: &AppHandle) {
    set_importing(app, false);
}

/// (size, reclaimable) in bytes from the query connection; zeros when it is unavailable.
pub fn db_sizes(app: &AppHandle) -> (i64, i64) {
    let st = app.state::<AppState>();
    let guard = st.query.lock().unwrap();
    match guard.as_ref() {
        Some(conn) => (
            db::file_size_bytes(conn).unwrap_or(0),
            db::reclaimable_bytes(conn).unwrap_or(0),
        ),
        None => (0, 0),
    }
}

/// Takes the importing flag; false if an import/vacuum is already running.
pub fn try_begin(app: &AppHandle) -> bool {
    let st = app.state::<AppState>();
    if st.importing.swap(true, Ordering::SeqCst) {
        return false;
    }
    let _ = app.emit("importing", true);
    true
}

pub fn reset_prune(app: &AppHandle) {
    *app.state::<AppState>().next_prune_at.lock().unwrap() = None;
}

pub fn status_dto(app: &AppHandle) -> StatusDto {
    let st = app.state::<AppState>();
    let db_path = st.settings.lock().unwrap().resolved_db_path().display().to_string();
    let status = st.status.lock().unwrap().clone();
    let min_mb = st.settings.lock().unwrap().auto_vacuum_min_db_mb;
    let (db_size_bytes, reclaimable_bytes) = db_sizes(app);
    StatusDto {
        status,
        importing: st.importing.load(Ordering::SeqCst),
        db_path,
        db_size_bytes,
        reclaimable_bytes,
        vacuum_recommended: should_vacuum(db_size_bytes, reclaimable_bytes, min_mb),
        version: app.package_info().version.to_string(),
    }
}

pub fn update_title(app: &AppHandle) {
    let path = app.state::<AppState>().settings.lock().unwrap().resolved_db_path();
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.set_title(&format!("VrcLogger \u{2014} {}", path.display()));
    }
}

/// (Re)opens the query connection from the current settings.
pub fn reopen_query_db(app: &AppHandle) -> Result<(), String> {
    let st = app.state::<AppState>();
    let path = st.settings.lock().unwrap().resolved_db_path();
    match open_db(&path) {
        Ok(conn) => {
            *st.query.lock().unwrap() = Some(conn);
            Ok(())
        }
        Err(e) => {
            *st.query.lock().unwrap() = None;
            Err(e.to_string())
        }
    }
}

pub fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

/// Shows (or creates) the settings window.
pub fn open_settings(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("settings") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
        return;
    }
    let built = WebviewWindowBuilder::new(app, "settings", WebviewUrl::App("index.html?window=settings".into()))
        .title("設定 — VrcLogger")
        .inner_size(760.0, 780.0)
        .min_inner_size(560.0, 480.0)
        .resizable(true)
        .center()
        .build();
    if let Ok(w) = built {
        let _ = w.remove_menu();
    }
}

pub fn exit_app(app: &AppHandle) {
    app.state::<AppState>().exit_requested.store(true, Ordering::SeqCst);
    app.exit(0);
}

/// Import (+ hourly prune). Mirrors `MainViewModel.RunImportAsync`.
pub async fn run_import(app: AppHandle, automatic: bool) {
    if !try_begin(&app) {
        return;
    }
    let (folders, db_path, noise, retention, photo_folders) = {
        let s = app.state::<AppState>().settings.lock().unwrap().clone();
        (
            s.resolved_log_folders(),
            s.resolved_db_path(),
            s.raw_log_noise_patterns.clone(),
            s.raw_log_retention_days,
            s.resolved_photo_folders(),
        )
    };
    let do_scan = !photo_folders.is_empty() && photos::scan_due(&app, automatic);
    let do_prune = app
        .state::<AppState>()
        .next_prune_at
        .lock()
        .unwrap()
        .is_none_or(|t| Instant::now() >= t);

    let app2 = app.clone();
    let result = tauri::async_runtime::spawn_blocking(move || -> Result<(bool, PhotosChanged), String> {
        // Import uses its own connection so query commands never collide with it.
        let conn = open_db(&db_path).map_err(|e| e.to_string())?;
        let svc = ImportService::with_options(&conn, ParseOptions::new(noise.clone()));
        let progress = |m: &str| set_status(&app2, m);
        let imported = svc
            .import_folders(&folders, Some(&progress as &dyn Fn(&str)))
            .map_err(|e| e.to_string())?
            .imported
            > 0;
        let mut pruned_total = 0;
        if do_prune {
            let pruned = svc.prune(retention, &noise, None).map_err(|e| e.to_string())?;
            pruned_total = pruned.total();
            if pruned_total > 0 {
                set_status(
                    &app2,
                    format!(
                        "生ログ整理: {} 期限切れ / {} ノイズ を削除",
                        pruned.expired_deleted, pruned.noise_deleted
                    ),
                );
            }
        }
        let photo_changes = photos::scan_step(&app2, &conn, &photo_folders, do_scan, imported);
        Ok((imported || pruned_total > 0, photo_changes))
    })
    .await;

    let mut photo_changes = PhotosChanged::default();
    let changed = match result {
        Ok(Ok((changed, pc))) => {
            photo_changes = pc;
            if do_prune {
                *app.state::<AppState>().next_prune_at.lock().unwrap() = Some(Instant::now() + PRUNE_INTERVAL);
            }
            changed
        }
        Ok(Err(msg)) => {
            set_status(&app, format!("取り込みエラー: {msg}"));
            true
        }
        Err(e) => {
            set_status(&app, format!("取り込みエラー: {e}"));
            true
        }
    };
    set_importing(&app, false);
    let _ = app.emit("import-finished", ImportFinished { changed, automatic });
    photos::emit_changed(&app, &photo_changes);
}

pub async fn reimport_all(app: AppHandle) {
    if app.state::<AppState>().importing.load(Ordering::SeqCst) {
        return;
    }
    reset_prune(&app);
    let db_path = app.state::<AppState>().settings.lock().unwrap().resolved_db_path();
    let r = tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
        let conn = open_db(&db_path).map_err(|e| e.to_string())?;
        let svc = ImportService::new(&conn);
        svc.backfill_avatar_events_from_raw_events().map_err(|e| e.to_string())?;
        svc.mark_all_for_reimport().map_err(|e| e.to_string())?;
        Ok(())
    })
    .await;
    match r {
        Ok(Ok(())) => run_import(app, false).await,
        Ok(Err(msg)) => set_status(&app, format!("取り込みエラー: {msg}")),
        Err(e) => set_status(&app, format!("取り込みエラー: {e}")),
    }
}

pub async fn vacuum(app: AppHandle) {
    if !try_begin(&app) {
        return;
    }
    let db_path = app.state::<AppState>().settings.lock().unwrap().resolved_db_path();
    set_status(&app, "DB 最適化中 (VACUUM)…");
    let r = tauri::async_runtime::spawn_blocking(move || -> Result<(i64, i64), String> {
        let conn = open_db(&db_path).map_err(|e| e.to_string())?;
        let before = db::file_size_bytes(&conn).map_err(|e| e.to_string())?;
        db::vacuum(&conn).map_err(|e| e.to_string())?;
        let after = db::file_size_bytes(&conn).map_err(|e| e.to_string())?;
        Ok((before, after))
    })
    .await;
    match r {
        Ok(Ok((b, a))) => set_status(&app, format!("DB 最適化完了: {} MB → {} MB", b / 1024 / 1024, a / 1024 / 1024)),
        Ok(Err(msg)) => set_status(&app, format!("DB 最適化エラー: {msg}")),
        Err(e) => set_status(&app, format!("DB 最適化エラー: {e}")),
    }
    set_importing(&app, false);
}

pub async fn export_viewer(app: AppHandle) {
    let (db_path, start_dir) = {
        let s = app.state::<AppState>().settings.lock().unwrap().clone();
        let dir = s
            .last_viewer_export_folder
            .as_deref()
            .map(std::path::PathBuf::from)
            .filter(|p| p.is_dir())
            .unwrap_or_else(settings_dir);
        (s.resolved_db_path(), dir)
    };
    let app_dlg = app.clone();
    let picked = tauri::async_runtime::spawn_blocking(move || {
        let mut b = app_dlg
            .dialog()
            .file()
            .set_title("閲覧用 HTML を書き出す")
            .add_filter("HTML ファイル", &["html"])
            .set_file_name("vrclog-viewer.html");
        if start_dir.is_dir() {
            b = b.set_directory(start_dir);
        }
        b.blocking_save_file()
    })
    .await
    .ok()
    .flatten();
    let Some(path) = picked.and_then(|p| p.into_path().ok()) else {
        return;
    };

    if let Some(folder) = path.parent() {
        let st = app.state::<AppState>();
        let mut s = st.settings.lock().unwrap();
        s.last_viewer_export_folder = Some(folder.display().to_string());
        let _ = s.save();
    }
    let out = path.clone();
    let r = tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
        let conn = open_db(&db_path).map_err(|e| e.to_string())?;
        export::export(&conn, &out, Local::now().naive_local()).map_err(|e| e.to_string())
    })
    .await;
    match r {
        Ok(Ok(())) => set_status(&app, format!("閲覧用 HTML を書き出しました: {}", path.display())),
        Ok(Err(msg)) => set_status(&app, format!("閲覧用 HTML の書き出しエラー: {msg}")),
        Err(e) => set_status(&app, format!("閲覧用 HTML の書き出しエラー: {e}")),
    }
}

/// Restarts the auto-import timer from the current settings (<= 0 minutes: no timer).
pub fn restart_timer(app: &AppHandle) {
    let st = app.state::<AppState>();
    let generation = st.timer_generation.fetch_add(1, Ordering::SeqCst) + 1;
    let minutes = st.settings.lock().unwrap().auto_import_interval_minutes;
    if minutes <= 0 {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(minutes as u64 * 60)).await;
            if app.state::<AppState>().timer_generation.load(Ordering::SeqCst) != generation {
                break;
            }
            run_import(app.clone(), true).await;
        }
    });
}
