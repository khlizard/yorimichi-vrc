//! Auto-update through Velopack. The feed is the project's GitHub Releases unless the settings
//! name another one (an http(s) URL or a local folder).

use std::sync::atomic::Ordering;
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use velopack::sources::{FileSource, HttpSource};
use velopack::{UpdateCheck, UpdateManager};

use crate::app::{self, AppState};

const FIRST_CHECK_DELAY: Duration = Duration::from_secs(30);
const CHECK_INTERVAL: Duration = Duration::from_secs(24 * 3600);
/// Assets of the newest GitHub release: `releases.win.json` and the full package it lists.
pub const DEFAULT_FEED: &str = "https://github.com/khlizard/yorimichi-vrc/releases/latest/download";

/// The feed to use: the one from the settings, or the default when that is blank.
fn effective_feed(setting: &str) -> String {
    match setting.trim() {
        "" => DEFAULT_FEED.to_string(),
        s => s.to_string(),
    }
}

#[derive(Serialize, Clone)]
struct UpdateReady {
    version: String,
}

/// What a check found. The About dialog shows it; the automatic check only acts on `Ready`.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum CheckResult {
    /// No newer version. `version` is the installed one.
    Latest { version: String },
    /// A newer version is downloaded and waits for a restart.
    Ready { version: String },
    /// Not a Velopack-installed build (dev run or portable copy), so it cannot update itself.
    NotInstalled,
    Failed { message: String },
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
            // Ask about a downloaded update once; do not ask again while it waits for a restart.
            let waiting = app.state::<AppState>().pending_update.lock().unwrap().is_some();
            if !waiting {
                if let CheckResult::Ready { version } = check(app.clone()).await {
                    let _ = app.emit("update-ready", UpdateReady { version });
                }
            }
            tokio::time::sleep(CHECK_INTERVAL).await;
        }
    });
}

/// Looks for a newer version and downloads it. An update already downloaded is reported as
/// `Ready` without checking again.
pub async fn check(app: AppHandle) -> CheckResult {
    if let Some(info) = app.state::<AppState>().pending_update.lock().unwrap().as_ref() {
        return CheckResult::Ready { version: info.TargetFullRelease.Version.to_string() };
    }
    let feed = effective_feed(&app.state::<AppState>().settings.lock().unwrap().update_feed);

    let r = tauri::async_runtime::spawn_blocking(move || -> Result<CheckResult, String> {
        let Ok(manager) = make_manager(&feed) else {
            return Ok(CheckResult::NotInstalled);
        };
        match manager.check_for_updates().map_err(|e| e.to_string())? {
            UpdateCheck::UpdateAvailable(info) => {
                let version = info.TargetFullRelease.Version.to_string();
                app::set_status(&app, format!("更新をダウンロード中: {version}"));
                manager.download_updates(&info, None).map_err(|e| e.to_string())?;
                *app.state::<AppState>().pending_update.lock().unwrap() = Some(*info);
                app::set_status(&app, format!("更新をダウンロードしました: {version}"));
                Ok(CheckResult::Ready { version })
            }
            _ => Ok(CheckResult::Latest { version: manager.get_current_version_as_string() }),
        }
    })
    .await;
    app::flatten_join(r).unwrap_or_else(|message| CheckResult::Failed { message })
}

/// Applies the downloaded update and restarts; on success the process exits and this does not return.
pub fn apply(app: &AppHandle) -> Result<(), String> {
    let st = app.state::<AppState>();
    let info = st.pending_update.lock().unwrap().clone().ok_or_else(|| "ダウンロード済みの更新がありません。".to_string())?;
    let feed = effective_feed(&st.settings.lock().unwrap().update_feed);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blank_setting_uses_the_default_feed() {
        assert_eq!(effective_feed(""), DEFAULT_FEED);
        assert_eq!(effective_feed("  \t"), DEFAULT_FEED);
        assert_eq!(effective_feed(" D:\\feed "), "D:\\feed");
        assert_eq!(effective_feed("https://example.com/f"), "https://example.com/f");
    }

    /// The About dialog reads this shape (`UpdateCheck` in ui/src/api.ts).
    #[test]
    fn check_result_json_matches_the_ui() {
        let json = |r: CheckResult| serde_json::to_string(&r).unwrap();
        assert_eq!(json(CheckResult::Latest { version: "1.0.0".into() }), r#"{"kind":"latest","version":"1.0.0"}"#);
        assert_eq!(json(CheckResult::Ready { version: "1.1.0".into() }), r#"{"kind":"ready","version":"1.1.0"}"#);
        assert_eq!(json(CheckResult::NotInstalled), r#"{"kind":"notInstalled"}"#);
        assert_eq!(json(CheckResult::Failed { message: "x".into() }), r#"{"kind":"failed","message":"x"}"#);
    }
}
