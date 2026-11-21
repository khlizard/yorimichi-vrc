//! Shared state and the long-running operations (import, vacuum, export, timer, window helpers).

use std::collections::HashSet;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use chrono::Local;
use rusqlite::Connection;
use serde::Serialize;
use tauri::menu::CheckMenuItem;
use tauri::{AppHandle, Emitter, Manager, Theme, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_dialog::DialogExt;
use vrclogger_core::data::db;
use vrclogger_core::data::import::ImportService;
use vrclogger_core::export;
use vrclogger_core::parsing::ParseOptions;

use crate::errors::user_message;
use crate::maintenance::should_vacuum;
use crate::photos::{self, PhotosChanged};
use crate::settings::{settings_dir, Settings, ThemeSetting};

const PRUNE_INTERVAL: Duration = Duration::from_secs(3600);

pub struct AppState {
    pub settings: Mutex<Settings>,
    /// Connection used by the query commands (None if the DB could not be opened).
    pub query: Mutex<Option<Connection>>,
    /// Second connection for photo lookups, so thumbnails are not held up by a long query on `query`.
    pub photo_query: Mutex<Option<Connection>>,
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
    /// Set when a shell thumbnail request hydrated an online-only file; disables that path for the session.
    pub shell_thumb_disabled: AtomicBool,
    /// Check items of the window menu's 表示 > テーマ submenu (kept in sync with `settings.theme`).
    pub theme_items: Mutex<Vec<(ThemeSetting, CheckMenuItem<tauri::Wry>)>>,
    /// Asks a running metadata write/remove batch to stop after the current file.
    pub metadata_cancel: AtomicBool,
}

impl AppState {
    pub fn new(settings: Settings) -> Self {
        Self {
            settings: Mutex::new(settings),
            query: Mutex::new(None),
            photo_query: Mutex::new(None),
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
            shell_thumb_disabled: AtomicBool::new(false),
            theme_items: Mutex::new(Vec::new()),
            metadata_cancel: AtomicBool::new(false),
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

/// Shown when a query command runs while the database could not be opened.
const NO_DB: &str = "データベースを開けません。設定の DB パスを確認してください。";

/// Locks the query connection and runs `f` on it; the error text is the user-facing one.
pub fn with_query_conn<T>(
    state: &AppState,
    f: impl FnOnce(&Connection) -> vrclogger_core::Result<T>,
) -> Result<T, String> {
    let guard = state.query.lock().unwrap();
    let conn = guard.as_ref().ok_or_else(|| NO_DB.to_string())?;
    f(conn).map_err(|e| user_message(&e))
}

/// Like [`with_query_conn`] on the connection kept for photo lookups (thumbnails, the context
/// menu's metadata status), so those never wait behind a long query on the main connection.
pub fn with_photo_conn<T>(
    state: &AppState,
    f: impl FnOnce(&Connection) -> vrclogger_core::Result<T>,
) -> Result<T, String> {
    let guard = state.photo_query.lock().unwrap();
    let conn = guard.as_ref().ok_or_else(|| NO_DB.to_string())?;
    f(conn).map_err(|e| user_message(&e))
}

/// Collapses the result of `spawn_blocking(|| -> Result<T, String>)`: a task that panicked or was
/// cancelled becomes an error string, like the task's own error.
pub fn flatten_join<T>(r: tauri::Result<Result<T, String>>) -> Result<T, String> {
    r.map_err(|e| e.to_string())?
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

/// The title carries no path: a default path contains the Windows user name, and titles end up
/// in screenshots. The settings window shows which database file is in use.
pub fn update_title(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.set_title("VrcLogger");
    }
}

/// (Re)opens the query connections (the main one and the one for photo lookups) from the current settings.
pub fn reopen_query_db(app: &AppHandle) -> Result<(), String> {
    let st = app.state::<AppState>();
    let path = st.settings.lock().unwrap().resolved_db_path();
    match open_db(&path).and_then(|query| Ok((query, open_db(&path)?))) {
        Ok((query, photo_query)) => {
            *st.query.lock().unwrap() = Some(query);
            *st.photo_query.lock().unwrap() = Some(photo_query);
            Ok(())
        }
        Err(e) => {
            *st.query.lock().unwrap() = None;
            *st.photo_query.lock().unwrap() = None;
            Err(user_message(&e))
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

fn native_theme(t: ThemeSetting) -> Option<Theme> {
    match t {
        ThemeSetting::Auto => None,
        ThemeSetting::Light => Some(Theme::Light),
        ThemeSetting::Dark => Some(Theme::Dark),
    }
}

/// Applies `settings.theme` to the native window chrome (main + settings) and the menu checks.
pub fn apply_theme(app: &AppHandle) {
    let st = app.state::<AppState>();
    let theme = st.settings.lock().unwrap().theme;
    let native = native_theme(theme);
    // The app-wide call also themes the menu bar; the per-window calls cover the windows that exist now.
    app.set_theme(native);
    for label in ["main", "settings"] {
        if let Some(w) = app.get_webview_window(label) {
            let _ = w.set_theme(native);
        }
    }
    for (t, item) in st.theme_items.lock().unwrap().iter() {
        let _ = item.set_checked(*t == theme);
    }
}

/// Window-menu choice: saves immediately, applies it and tells every window.
pub fn choose_theme(app: &AppHandle, theme: ThemeSetting) {
    let st = app.state::<AppState>();
    let (snapshot, saved) = {
        let mut s = st.settings.lock().unwrap();
        s.theme = theme;
        let saved = s.save();
        (s.clone(), saved)
    };
    apply_theme(app);
    if let Err(e) = saved {
        set_status(app, format!("設定を保存できません: {e}"));
    }
    let _ = app.emit("settings-changed", snapshot);
}

/// Shows (or creates) the settings window.
pub fn open_settings(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("settings") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
        return;
    }
    let theme = native_theme(app.state::<AppState>().settings.lock().unwrap().theme);
    let built = WebviewWindowBuilder::new(app, "settings", WebviewUrl::App("index.html?window=settings".into()))
        .title("設定 — VrcLogger")
        .theme(theme)
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

/// Import (+ hourly prune).
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
        let conn = open_db(&db_path).map_err(|e| user_message(&e))?;
        let svc = ImportService::with_options(&conn, ParseOptions::new(noise.clone()));
        let progress = |m: &str| set_status(&app2, m);
        let imported = svc
            .import_folders(&folders, Some(&progress as &dyn Fn(&str)))
            .map_err(|e| user_message(&e))?
            .imported
            > 0;
        let mut pruned_total = 0;
        if do_prune {
            let pruned = svc.prune(retention, &noise, None).map_err(|e| user_message(&e))?;
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
    let changed = match flatten_join(result) {
        Ok((changed, pc)) => {
            photo_changes = pc;
            if do_prune {
                *app.state::<AppState>().next_prune_at.lock().unwrap() = Some(Instant::now() + PRUNE_INTERVAL);
            }
            changed
        }
        Err(msg) => {
            set_status(&app, format!("取り込みエラー: {msg}"));
            true
        }
    };
    set_importing(&app, false);
    let _ = app.emit("import-finished", ImportFinished { changed, automatic });
    photos::emit_changed(&app, &photo_changes);
}

pub async fn reimport_all(app: AppHandle) {
    if app.state::<AppState>().importing.load(Ordering::SeqCst) {
        set_status(&app, "ほかの処理の実行中です。終わってから、もう一度「再取り込み」を実行してください。");
        return;
    }
    reset_prune(&app);
    let db_path = app.state::<AppState>().settings.lock().unwrap().resolved_db_path();
    let r = tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
        let conn = open_db(&db_path).map_err(|e| user_message(&e))?;
        let svc = ImportService::new(&conn);
        svc.backfill_avatar_events_from_raw_events().map_err(|e| user_message(&e))?;
        svc.mark_all_for_reimport().map_err(|e| user_message(&e))?;
        Ok(())
    })
    .await;
    match flatten_join(r) {
        Ok(()) => run_import(app, false).await,
        Err(msg) => set_status(&app, format!("取り込みエラー: {msg}")),
    }
}

pub async fn vacuum(app: AppHandle) {
    if !try_begin(&app) {
        return;
    }
    let db_path = app.state::<AppState>().settings.lock().unwrap().resolved_db_path();
    set_status(&app, "DB 最適化中 (VACUUM)…");
    let r = tauri::async_runtime::spawn_blocking(move || -> Result<(i64, i64), String> {
        let conn = open_db(&db_path).map_err(|e| user_message(&e))?;
        let before = db::file_size_bytes(&conn).map_err(|e| user_message(&e))?;
        db::vacuum(&conn).map_err(|e| user_message(&e))?;
        let after = db::file_size_bytes(&conn).map_err(|e| user_message(&e))?;
        Ok((before, after))
    })
    .await;
    match flatten_join(r) {
        Ok((b, a)) => set_status(&app, format!("DB 最適化完了: {} MB → {} MB", b / 1024 / 1024, a / 1024 / 1024)),
        Err(msg) => set_status(&app, format!("DB 最適化エラー: {msg}")),
    }
    set_importing(&app, false);
}

/// Folder the "save HTML" dialogs open in: where the last export went, else Documents.
pub fn export_start_dir(s: &Settings) -> std::path::PathBuf {
    s.last_viewer_export_folder
        .as_deref()
        .map(std::path::PathBuf::from)
        .filter(|p| p.is_dir())
        .or_else(dirs::document_dir)
        .unwrap_or_else(settings_dir)
}

/// Asks where to save an HTML export (the dialog opens in [`export_start_dir`]) and remembers the
/// chosen folder as `last_viewer_export_folder`. `None` when the dialog was cancelled.
pub async fn pick_html_save_path(app: &AppHandle, title: &'static str, file_name: String) -> Option<std::path::PathBuf> {
    let start_dir = export_start_dir(&app.state::<AppState>().settings.lock().unwrap());
    let app_dlg = app.clone();
    let picked = tauri::async_runtime::spawn_blocking(move || {
        let mut b = app_dlg
            .dialog()
            .file()
            .set_title(title)
            .add_filter("HTML ファイル", &["html"])
            .set_file_name(file_name);
        if start_dir.is_dir() {
            b = b.set_directory(start_dir);
        }
        b.blocking_save_file()
    })
    .await
    .ok()
    .flatten();
    let path = picked.and_then(|p| p.into_path().ok())?;
    if let Some(folder) = path.parent() {
        let st = app.state::<AppState>();
        let mut s = st.settings.lock().unwrap();
        s.last_viewer_export_folder = Some(folder.display().to_string());
        let _ = s.save();
    }
    Some(path)
}

pub async fn export_viewer(app: AppHandle) {
    let db_path = app.state::<AppState>().settings.lock().unwrap().resolved_db_path();
    let Some(path) = pick_html_save_path(&app, "閲覧用 HTML を書き出す", "vrclog-viewer.html".to_string()).await else {
        return;
    };
    let out = path.clone();
    let r = tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
        let conn = open_db(&db_path).map_err(|e| user_message(&e))?;
        export::export(&conn, &out, Local::now().naive_local()).map_err(|e| user_message(&e))
    })
    .await;
    match flatten_join(r) {
        Ok(()) => set_status(&app, format!("閲覧用 HTML を書き出しました: {}", path.display())),
        Err(msg) => set_status(&app, format!("閲覧用 HTML の書き出しエラー: {msg}")),
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
