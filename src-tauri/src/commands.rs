//! Tauri commands for the app itself: status, import, settings, updates, clipboard.
//! JS argument names are camelCase (Tauri converts them to these snake_case names).
//! Log searches are in `log_commands`, photos in `photo_commands`.

use std::time::Duration;

use chrono::{Local, NaiveDate};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_autostart::ManagerExt as _;
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;

use crate::app::{self, AppState, StatusDto};
use crate::settings::Settings;
use crate::{maintenance, update};

pub(crate) type CmdResult<T> = Result<T, String>;

pub(crate) fn parse_date(s: Option<String>) -> CmdResult<Option<NaiveDate>> {
    match s.as_deref().map(str::trim) {
        None | Some("") => Ok(None),
        Some(v) => NaiveDate::parse_from_str(v, "%Y-%m-%d")
            .map(Some)
            .map_err(|_| format!("日付の形式が正しくありません: {v}")),
    }
}

/// Keyset-paging position from the two JS arguments; both must be present.
pub(crate) fn keyset(time: Option<String>, id: Option<i64>) -> Option<(String, i64)> {
    match (time, id) {
        (Some(t), Some(id)) => Some((t, id)),
        _ => None,
    }
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
    app::flatten_join(
        tauri::async_runtime::spawn_blocking(move || {
            write_clipboard(|c| c.set_text(text.as_str()))
        })
        .await,
    )
}

/// Runs a clipboard write, retrying while the clipboard is held by another app. The user's IME
/// (ATOK) locks the clipboard right after each copy, so a failure is retried every 100 ms, up
/// to 20 attempts. `write` borrows its data, so a retry costs no copy.
pub(crate) fn write_clipboard(mut write: impl FnMut(&mut arboard::Clipboard) -> Result<(), arboard::Error>) -> CmdResult<()> {
    const ATTEMPTS: u32 = 20;
    for attempt in 0..ATTEMPTS {
        if arboard::Clipboard::new().and_then(|mut c| write(&mut c)).is_ok() {
            return Ok(());
        }
        if attempt + 1 < ATTEMPTS {
            std::thread::sleep(Duration::from_millis(100));
        }
    }
    Err("クリップボードが他のアプリ (IME のクリップボード監視など) に使用中のため、コピーできませんでした。もう一度お試しください。".to_string())
}

#[tauri::command]
pub async fn open_url(app: AppHandle, url: String) -> CmdResult<()> {
    let lower = url.to_ascii_lowercase();
    if !(lower.starts_with("http://") || lower.starts_with("https://")) {
        return Err("http / https 以外の URL は開けません。".into());
    }
    app.opener().open_url(url, None::<&str>).map_err(|e| e.to_string())
}
