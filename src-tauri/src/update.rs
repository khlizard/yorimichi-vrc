//! Auto-update through Velopack (feed = http(s) URL or a local folder from settings).

use std::sync::atomic::Ordering;
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use velopack::sources::{FileSource, HttpSource};
use velopack::{UpdateCheck, UpdateInfo, UpdateManager};

use crate::app::{self, AppState};

const FIRST_CHECK_DELAY: Duration = Duration::from_secs(30);
const CHECK_INTERVAL: Duration = Duration::from_secs(24 * 3600);
pub const NO_FEED_MSG: &str = "更新の配布元が設定されていません。設定で指定してください。";

#[derive(Serialize, Clone)]
struct UpdateReady {
    version: String,
}

fn make_manager(feed: &str) -> Result<UpdateManager, velopack::Error> {
    let lower = feed.to_ascii_lowercase();
    if lower.starts_with("http://") || lower.starts_with("https://") {
        UpdateManager::new(HttpSource::new(feed), None, None)
    } else {
        UpdateManager::new(FileSource::new(feed), None, None)
    }
}

/// Spawns the automatic check loop (call once from setup).
pub fn start(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(FIRST_CHECK_DELAY).await;
        loop {
            check(app.clone(), false).await;
            tokio::time::sleep(CHECK_INTERVAL).await;
        }
    });
}

pub async fn check(app: AppHandle, manual: bool) {
    let feed = app.state::<AppState>().settings.lock().unwrap().update_feed.trim().to_string();
    if feed.is_empty() {
        if manual {
            app::set_status(&app, NO_FEED_MSG);
        }
        return;
    }
    // Do not re-download while an update is already waiting for a restart.
    if let Some(info) = app.state::<AppState>().pending_update.lock().unwrap().as_ref() {
        if manual {
            let _ = app.emit("update-ready", UpdateReady { version: info.TargetFullRelease.Version.to_string() });
        }
        return;
    }

    let app2 = app.clone();
    let r = tauri::async_runtime::spawn_blocking(move || -> Result<Option<UpdateInfo>, String> {
        let Ok(manager) = make_manager(&feed) else {
            // Not a Velopack-installed build (e.g. dev run or portable copy).
            if manual {
                app::set_status(&app2, "インストール版でのみ更新できます");
            }
            return Ok(None);
        };
        match manager.check_for_updates() {
            Ok(UpdateCheck::UpdateAvailable(info)) => {
                app::set_status(&app2, format!("更新をダウンロード中: {}", info.TargetFullRelease.Version));
                manager.download_updates(&info, None).map_err(|e| e.to_string())?;
                Ok(Some(*info))
            }
            Ok(_) => {
                if manual {
                    app::set_status(&app2, format!("最新版です ({})", manager.get_current_version_as_string()));
                }
                Ok(None)
            }
            Err(e) => Err(e.to_string()),
        }
    })
    .await;

    match r {
        Ok(Ok(Some(info))) => {
            let version = info.TargetFullRelease.Version.to_string();
            *app.state::<AppState>().pending_update.lock().unwrap() = Some(info);
            app::set_status(&app, format!("更新をダウンロードしました: {version}"));
            let _ = app.emit("update-ready", UpdateReady { version });
        }
        Ok(Ok(None)) => {}
        Ok(Err(msg)) => {
            if manual {
                app::set_status(&app, format!("更新の確認エラー: {msg}"));
            }
        }
        Err(e) => {
            if manual {
                app::set_status(&app, format!("更新の確認エラー: {e}"));
            }
        }
    }
}

/// Applies the downloaded update and restarts; on success the process exits and this does not return.
pub fn apply(app: &AppHandle) -> Result<(), String> {
    let st = app.state::<AppState>();
    let info = st.pending_update.lock().unwrap().clone().ok_or_else(|| "ダウンロード済みの更新がありません。".to_string())?;
    let feed = st.settings.lock().unwrap().update_feed.trim().to_string();
    let manager = make_manager(&feed).map_err(|e| e.to_string())?;
    st.exit_requested.store(true, Ordering::SeqCst);
    // The Velopack call exits the process itself, bypassing RunEvent::Exit, so close the DB first.
    crate::close_query_db(app);
    if let Err(e) = manager.apply_updates_and_restart(&info) {
        st.exit_requested.store(false, Ordering::SeqCst);
        let _ = app::reopen_query_db(app);
        return Err(e.to_string());
    }
    Ok(())
}
