//! Tauri commands. JS argument names are camelCase (Tauri converts them to these snake_case names).

use std::time::Duration;

use chrono::{Local, NaiveDate};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_autostart::ManagerExt as _;
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;
use vrclogger_core::data::dtos::*;
use vrclogger_core::data::photo_query::{DayCount, PhotoDetail, PhotoFilter, PhotoRow, PhotoService};
use vrclogger_core::data::query::QueryService;

use crate::app::{self, AppState, StatusDto};
use crate::settings::Settings;
use crate::{maintenance, update};

pub(crate) type CmdResult<T> = Result<T, String>;

const ALL_LABEL: &str = "(全て)";

pub(crate) fn parse_date(s: Option<String>) -> CmdResult<Option<NaiveDate>> {
    match s.as_deref().map(str::trim) {
        None | Some("") => Ok(None),
        Some(v) => NaiveDate::parse_from_str(v, "%Y-%m-%d")
            .map(Some)
            .map_err(|_| format!("日付の形式が正しくありません: {v}")),
    }
}

/// "" / "(全て)" / None -> None.
fn filter_opt(s: Option<String>) -> Option<String> {
    s.filter(|v| !v.is_empty() && v != ALL_LABEL)
}

fn with_query<T>(
    state: &AppState,
    f: impl FnOnce(&QueryService) -> vrclogger_core::Result<T>,
) -> CmdResult<T> {
    let guard = state.query.lock().unwrap();
    let conn = guard.as_ref().ok_or_else(|| "データベースを開けません。設定の DB パスを確認してください。".to_string())?;
    f(&QueryService::new(conn)).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_status(app: AppHandle) -> CmdResult<StatusDto> {
    Ok(app::status_dto(&app))
}

#[tauri::command]
pub async fn import_now(app: AppHandle) -> CmdResult<()> {
    app::run_import(app, false).await;
    Ok(())
}

#[tauri::command]
pub async fn reimport_all(app: AppHandle) -> CmdResult<()> {
    app::reimport_all(app).await;
    Ok(())
}

#[tauri::command]
pub async fn vacuum(app: AppHandle) -> CmdResult<()> {
    app::vacuum(app).await;
    Ok(())
}

#[tauri::command]
pub async fn export_viewer(app: AppHandle) -> CmdResult<()> {
    app::export_viewer(app).await;
    Ok(())
}

#[tauri::command]
pub async fn get_settings(state: State<'_, AppState>) -> CmdResult<Settings> {
    Ok(state.settings.lock().unwrap().clone())
}

#[tauri::command]
pub async fn save_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    settings: Settings,
    start_with_windows: bool,
) -> CmdResult<String> {
    let db_changed;
    let photos_changed;
    {
        let mut cur = state.settings.lock().unwrap();
        db_changed = cur.db_path != settings.db_path;
        cur.log_folders =
            settings.log_folders.iter().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
        cur.db_path = settings.db_path;
        cur.auto_import_interval_minutes = settings.auto_import_interval_minutes.max(0);
        cur.minimize_to_tray = settings.minimize_to_tray;
        // Saving from the settings window is an explicit choice about staying in the tray.
        cur.tray_choice_made = true;
        cur.raw_log_retention_days = settings.raw_log_retention_days.max(0);
        cur.raw_log_noise_patterns =
            settings.raw_log_noise_patterns.iter().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
        let new_photo_folders: Vec<String> =
            settings.photo_folders.iter().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
        photos_changed = cur.photo_folders != new_photo_folders;
        cur.photo_folders = new_photo_folders;
        cur.day_start_hour = settings.day_start_hour.min(23);
        cur.show_first_met = settings.show_first_met;
        cur.reunion_days = settings.reunion_days.max(0);
        cur.auto_vacuum = settings.auto_vacuum;
        cur.auto_vacuum_min_db_mb = settings.auto_vacuum_min_db_mb.max(0);
        cur.backup_folder = settings.backup_folder.trim().to_string();
        cur.backup_interval_days = settings.backup_interval_days.max(0);
        cur.backup_keep_count = settings.backup_keep_count.max(1);
        cur.update_feed = settings.update_feed.trim().to_string();
        cur.theme = settings.theme;
        // LastViewerExportFolder and LastBackupAt are owned by the backend; keep the stored values.
        cur.save()?;
        let _ = app.emit("settings-changed", cur.clone());
    }
    if db_changed {
        if let Err(e) = app::reopen_query_db(&app) {
            app::set_status(&app, format!("データベースを開けません: {e}"));
        }
        app::update_title(&app);
    }
    app::apply_theme(&app);
    app::reset_prune(&app);
    app::restart_timer(&app);
    if photos_changed {
        *state.next_photo_scan_at.lock().unwrap() = None;
        tauri::async_runtime::spawn(app::run_import(app.clone(), false));
    }

    let al = app.autolaunch();
    let result = (|| -> Result<(), String> {
        let enabled = al.is_enabled().map_err(|e| e.to_string())?;
        if start_with_windows {
            al.enable().map_err(|e| e.to_string())?;
        } else if enabled {
            al.disable().map_err(|e| e.to_string())?;
        }
        Ok(())
    })();
    Ok(match result {
        Ok(()) => format!("保存しました ({})", Local::now().format("%H:%M:%S")),
        Err(msg) => format!("保存しました。自動起動の設定に失敗: {msg}"),
    })
}

#[tauri::command]
pub async fn default_noise_patterns() -> CmdResult<Vec<String>> {
    Ok(vrclogger_core::parsing::noise::default_patterns())
}

#[tauri::command]
pub async fn pick_folder(app: AppHandle, title: Option<String>) -> CmdResult<Option<String>> {
    let title = title.unwrap_or_else(|| "フォルダを選択".to_string());
    let picked = tauri::async_runtime::spawn_blocking(move || {
        app.dialog().file().set_title(title).blocking_pick_folder()
    })
    .await
    .map_err(|e| e.to_string())?;
    Ok(picked.and_then(|p| p.into_path().ok()).map(|p| p.display().to_string()))
}

#[tauri::command]
pub async fn answer_tray_choice(app: AppHandle, state: State<'_, AppState>, stay: bool) -> CmdResult<()> {
    let saved = {
        let mut s = state.settings.lock().unwrap();
        s.minimize_to_tray = stay;
        s.tray_choice_made = true;
        s.save()?;
        s.clone()
    };
    let _ = app.emit("settings-changed", saved);
    if stay {
        if let Some(w) = app.get_webview_window("main") {
            let _ = w.hide();
        }
    } else {
        app::exit_app(&app);
    }
    Ok(())
}

#[tauri::command]
pub async fn backup_now(app: AppHandle) -> CmdResult<()> {
    maintenance::backup_now(app, false).await;
    Ok(())
}

#[tauri::command]
pub async fn check_update(app: AppHandle) -> CmdResult<()> {
    update::check(app, true).await;
    Ok(())
}

#[tauri::command]
pub async fn apply_update(app: AppHandle) -> CmdResult<()> {
    update::apply(&app)
}

#[tauri::command]
pub async fn autostart_get(app: AppHandle) -> CmdResult<bool> {
    app.autolaunch().is_enabled().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn copy_text(text: String) -> CmdResult<()> {
    // The user's IME (ATOK) locks the clipboard right after each copy, so retry.
    tauri::async_runtime::spawn_blocking(move || {
        for attempt in 0..20 {
            if arboard::Clipboard::new().and_then(|mut c| c.set_text(text.clone())).is_ok() {
                return Ok(());
            }
            if attempt < 19 {
                std::thread::sleep(Duration::from_millis(100));
            }
        }
        Err("クリップボードが他のアプリ (IME のクリップボード監視など) に使用中のため、コピーできませんでした。もう一度お試しください。".to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn open_url(app: AppHandle, url: String) -> CmdResult<()> {
    let lower = url.to_ascii_lowercase();
    if !(lower.starts_with("http://") || lower.starts_with("https://")) {
        return Err("http / https 以外の URL は開けません。".into());
    }
    app.opener().open_url(url, None::<&str>).map_err(|e| e.to_string())
}

// ---- Queries ----

#[tauri::command]
pub async fn recent_players(
    state: State<'_, AppState>,
    from: Option<String>,
    to: Option<String>,
    offset: i64,
    limit: i64,
) -> CmdResult<Vec<PlayerHit>> {
    let (from, to) = (parse_date(from)?, parse_date(to)?);
    with_query(&state, |q| q.get_recent_players(from, to, offset, limit))
}

#[tauri::command]
pub async fn search_players(
    state: State<'_, AppState>,
    term: String,
    offset: i64,
    limit: i64,
) -> CmdResult<Vec<PlayerHit>> {
    with_query(&state, |q| q.search_players(&term, offset, limit))
}

#[tauri::command]
pub async fn encounters(
    state: State<'_, AppState>,
    player_id: i64,
    from: Option<String>,
    to: Option<String>,
) -> CmdResult<Vec<EncounterRow>> {
    let (from, to) = (parse_date(from)?, parse_date(to)?);
    with_query(&state, |q| q.get_encounters(player_id, from, to))
}

#[tauri::command]
pub async fn visits(
    state: State<'_, AppState>,
    world_term: Option<String>,
    from: Option<String>,
    to: Option<String>,
) -> CmdResult<Vec<VisitRow>> {
    let (from, to) = (parse_date(from)?, parse_date(to)?);
    with_query(&state, |q| q.get_visits(world_term.as_deref(), from, to))
}

#[tauri::command]
pub async fn visit_players(state: State<'_, AppState>, visit_id: i64) -> CmdResult<Vec<VisitPlayerRow>> {
    with_query(&state, |q| q.get_visit_players(visit_id))
}

#[tauri::command]
pub async fn visit_urls(state: State<'_, AppState>, visit_id: i64) -> CmdResult<Vec<UrlHit>> {
    with_query(&state, |q| q.get_visit_urls(visit_id))
}

#[tauri::command]
pub async fn search_urls(
    state: State<'_, AppState>,
    term: String,
    kind: Option<String>,
    from: Option<String>,
    to: Option<String>,
) -> CmdResult<Vec<UrlHit>> {
    let (from, to) = (parse_date(from)?, parse_date(to)?);
    let kind = filter_opt(kind);
    let mut rows = with_query(&state, |q| q.search_urls(&term, kind.as_deref(), from, to))?;
    rows.truncate(2000);
    Ok(rows)
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn search_raw_events(
    state: State<'_, AppState>,
    term: String,
    level: Option<String>,
    from: Option<String>,
    to: Option<String>,
    before_time: Option<String>,
    before_id: Option<i64>,
    limit: i64,
) -> CmdResult<Vec<RawEventHit>> {
    let (from, to) = (parse_date(from)?, parse_date(to)?);
    let level = filter_opt(level);
    let before = match (before_time, before_id) {
        (Some(t), Some(id)) => Some((t, id)),
        _ => None,
    };
    with_query(&state, |q| q.search_raw_events(&term, level.as_deref(), from, to, before, limit))
}

#[tauri::command]
pub async fn search_avatars(
    state: State<'_, AppState>,
    term: String,
    from: Option<String>,
    to: Option<String>,
) -> CmdResult<Vec<AvatarRow>> {
    let (from, to) = (parse_date(from)?, parse_date(to)?);
    with_query(&state, |q| q.search_avatars(&term, from, to))
}

#[tauri::command]
pub async fn search_notifications(
    state: State<'_, AppState>,
    term: String,
    kind: Option<String>,
    from: Option<String>,
    to: Option<String>,
) -> CmdResult<Vec<NotificationRow>> {
    let (from, to) = (parse_date(from)?, parse_date(to)?);
    let kind = filter_opt(kind);
    with_query(&state, |q| q.search_notifications(&term, kind.as_deref(), from, to))
}

#[tauri::command]
pub async fn notification_kinds(state: State<'_, AppState>) -> CmdResult<Vec<String>> {
    with_query(&state, |q| q.get_notification_kinds())
}

#[tauri::command]
pub async fn search_moderation(
    state: State<'_, AppState>,
    term: String,
    from: Option<String>,
    to: Option<String>,
) -> CmdResult<Vec<ModerationRow>> {
    let (from, to) = (parse_date(from)?, parse_date(to)?);
    with_query(&state, |q| q.search_moderation_events(&term, from, to))
}

// ---- Photos ----

fn with_photos<T>(
    state: &AppState,
    f: impl FnOnce(&PhotoService) -> vrclogger_core::Result<T>,
) -> CmdResult<T> {
    let guard = state.query.lock().unwrap();
    let conn = guard.as_ref().ok_or_else(|| "データベースを開けません。設定の DB パスを確認してください。".to_string())?;
    f(&PhotoService::new(conn)).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn rescan_photos(app: AppHandle) -> CmdResult<()> {
    app::run_import(app, false).await;
    Ok(())
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn photos(
    state: State<'_, AppState>,
    from: Option<String>,
    to: Option<String>,
    world_ref_id: Option<i64>,
    player_id: Option<i64>,
    visit_id: Option<i64>,
    favorite_only: bool,
    before_time: Option<String>,
    before_id: Option<i64>,
    limit: usize,
) -> CmdResult<Vec<PhotoRow>> {
    let filter = PhotoFilter {
        from: parse_date(from)?,
        to: parse_date(to)?,
        world_ref_id,
        player_id,
        visit_id,
        favorite_only,
        include_missing: false,
    };
    let before = match (before_time, before_id) {
        (Some(t), Some(id)) => Some((t, id)),
        _ => None,
    };
    with_photos(&state, |p| p.photos(&filter, before, limit.clamp(1, 500)))
}

#[tauri::command]
pub async fn photo_days(
    state: State<'_, AppState>,
    from: Option<String>,
    to: Option<String>,
    world_ref_id: Option<i64>,
    player_id: Option<i64>,
    favorite_only: bool,
) -> CmdResult<Vec<DayCount>> {
    let filter = PhotoFilter {
        from: parse_date(from)?,
        to: parse_date(to)?,
        world_ref_id,
        player_id,
        favorite_only,
        ..Default::default()
    };
    let hour = state.settings.lock().unwrap().day_start_hour;
    with_photos(&state, |p| p.photo_days(&filter, hour))
}

#[tauri::command]
pub async fn photos_for_visit(state: State<'_, AppState>, visit_id: i64) -> CmdResult<Vec<PhotoRow>> {
    with_photos(&state, |p| p.photos_for_visit(visit_id))
}

#[tauri::command]
pub async fn photo_detail(state: State<'_, AppState>, id: i64) -> CmdResult<Option<PhotoDetail>> {
    with_photos(&state, |p| p.photo_detail(id))
}

#[tauri::command]
pub async fn set_photo_favorite(state: State<'_, AppState>, id: i64, favorite: bool) -> CmdResult<()> {
    with_photos(&state, |p| p.set_favorite(id, favorite))
}

#[tauri::command]
pub async fn photo_count(state: State<'_, AppState>) -> CmdResult<i64> {
    with_photos(&state, |p| p.count())
}

fn photo_path_of(state: &AppState, id: i64) -> CmdResult<String> {
    with_photos(state, |p| p.photo_path(id))?.ok_or_else(|| "ファイルが見つかりません".to_string())
}

#[tauri::command]
pub async fn reveal_photo(app: AppHandle, state: State<'_, AppState>, id: i64) -> CmdResult<()> {
    let path = photo_path_of(&state, id)?;
    app.opener().reveal_item_in_dir(path).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn open_photo(app: AppHandle, state: State<'_, AppState>, id: i64) -> CmdResult<()> {
    let path = photo_path_of(&state, id)?;
    app.opener().open_path(path, None::<&str>).map_err(|e| e.to_string())
}

/// File facts for the photo info dialog. Only attributes are read (never the content).
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PhotoFileInfo {
    pub path: String,
    pub folder: String,
    pub exists: bool,
    pub online_only: bool,
    pub size: Option<u64>,
    /// Local time `YYYY-MM-DD HH:MM:SS`.
    pub modified: Option<String>,
    pub read_only: bool,
}

#[tauri::command]
pub async fn photo_file_info(state: State<'_, AppState>, id: i64) -> CmdResult<PhotoFileInfo> {
    let path = photo_path_of(&state, id)?;
    let p = std::path::Path::new(&path);
    let folder = p.parent().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default();
    Ok(match std::fs::metadata(p) {
        Ok(md) => PhotoFileInfo {
            online_only: vrclogger_core::photos::is_online_only(&md),
            size: Some(md.len()),
            modified: md
                .modified()
                .ok()
                .map(|t| chrono::DateTime::<Local>::from(t).format("%Y-%m-%d %H:%M:%S").to_string()),
            read_only: md.permissions().readonly(),
            exists: true,
            path,
            folder,
        },
        Err(_) => PhotoFileInfo { path, folder, exists: false, online_only: false, size: None, modified: None, read_only: false },
    })
}

/// Copies the full-size image to the clipboard. Online-only files are refused (no download).
#[tauri::command]
pub async fn copy_photo_image(state: State<'_, AppState>, id: i64) -> CmdResult<()> {
    let path = photo_path_of(&state, id)?;
    tauri::async_runtime::spawn_blocking(move || {
        let p = std::path::Path::new(&path);
        let md = std::fs::metadata(p).map_err(|_| "ファイルが見つかりません".to_string())?;
        if vrclogger_core::photos::is_online_only(&md) {
            return Err("オンライン専用のファイルはコピーできません（ダウンロードされていません）。".to_string());
        }
        let (width, height, bytes) =
            vrclogger_core::photos::rgba_pixels(p).map_err(|e| format!("画像を読み込めませんでした: {e}"))?;
        let image = arboard::ImageData { width: width as usize, height: height as usize, bytes: bytes.into() };
        // The user's IME (ATOK) locks the clipboard right after each copy, so retry.
        for attempt in 0..20 {
            if arboard::Clipboard::new().and_then(|mut c| c.set_image(image.clone())).is_ok() {
                return Ok(());
            }
            if attempt < 19 {
                std::thread::sleep(Duration::from_millis(100));
            }
        }
        Err("クリップボードが他のアプリ (IME のクリップボード監視など) に使用中のため、コピーできませんでした。もう一度お試しください。".to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}
