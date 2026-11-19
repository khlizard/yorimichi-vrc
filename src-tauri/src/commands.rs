//! Tauri commands. JS argument names are camelCase (Tauri converts them to these snake_case names).

use std::time::Duration;

use chrono::{Local, NaiveDate};
use tauri::{AppHandle, State};
use tauri_plugin_autostart::ManagerExt as _;
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;
use vrclogger_core::data::dtos::*;
use vrclogger_core::data::query::QueryService;

use crate::app::{self, AppState, StatusDto};
use crate::settings::Settings;

type CmdResult<T> = Result<T, String>;

const ALL_LABEL: &str = "(全て)";

fn parse_date(s: Option<String>) -> CmdResult<Option<NaiveDate>> {
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
    {
        let mut cur = state.settings.lock().unwrap();
        db_changed = cur.db_path != settings.db_path;
        cur.log_folders =
            settings.log_folders.iter().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
        cur.db_path = settings.db_path;
        cur.auto_import_interval_minutes = settings.auto_import_interval_minutes.max(0);
        cur.minimize_to_tray = settings.minimize_to_tray;
        cur.raw_log_retention_days = settings.raw_log_retention_days.max(0);
        cur.raw_log_noise_patterns =
            settings.raw_log_noise_patterns.iter().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
        // LastViewerExportFolder is owned by the backend (export dialog); keep the stored value.
        cur.save()?;
    }
    if db_changed {
        if let Err(e) = app::reopen_query_db(&app) {
            app::set_status(&app, format!("データベースを開けません: {e}"));
        }
        app::update_title(&app);
    }
    app::reset_prune(&app);
    app::restart_timer(&app);

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
pub async fn pick_folder(app: AppHandle) -> CmdResult<Option<String>> {
    let picked = tauri::async_runtime::spawn_blocking(move || {
        app.dialog().file().set_title("ログフォルダを選択").blocking_pick_folder()
    })
    .await
    .map_err(|e| e.to_string())?;
    Ok(picked.and_then(|p| p.into_path().ok()).map(|p| p.display().to_string()))
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
