//! Idle-time maintenance: automatic VACUUM and scheduled backups.

use std::io;
use std::path::Path;
use std::sync::atomic::Ordering;
use std::time::Duration;

use chrono::{Local, NaiveDateTime};
use tauri::{AppHandle, Emitter, Manager};
use vrclogger_core::data::db::{self, TS_FORMAT};

use crate::app::{self, AppState};
use crate::errors::user_message;
use crate::idle;

const IDLE_REQUIRED_SECS: u64 = 600;
const CPU_BUSY_LIMIT: f32 = 0.20;
const CPU_SAMPLE: Duration = Duration::from_secs(5);
const CHECK_INTERVAL: Duration = Duration::from_secs(60);

/// Vacuum only pays off for a big file with a sizeable share of free pages.
pub fn should_vacuum(db_size_bytes: i64, reclaimable_bytes: i64, min_db_mb: i64) -> bool {
    if db_size_bytes <= 0 {
        return false;
    }
    let min_bytes = min_db_mb.max(0).saturating_mul(1024 * 1024);
    db_size_bytes >= min_bytes && (reclaimable_bytes as i128) * 5 >= db_size_bytes as i128
}

/// Interval 0 disables automatic backups; a missing or unreadable timestamp counts as "never backed up".
pub fn backup_due(last_backup_at: Option<&str>, now: NaiveDateTime, interval_days: i32) -> bool {
    if interval_days <= 0 {
        return false;
    }
    let Some(last) = last_backup_at.and_then(|s| NaiveDateTime::parse_from_str(s.trim(), TS_FORMAT).ok()) else {
        return true;
    };
    now - last >= chrono::Duration::days(i64::from(interval_days))
}

pub fn backup_file_name(now: NaiveDateTime) -> String {
    format!("vrclog-{}.db", now.format("%Y%m%d-%H%M%S"))
}

fn is_backup_name(name: &str) -> bool {
    name.starts_with("vrclog-") && name.ends_with(".db") && name.len() > "vrclog-.db".len()
}

/// Deletes the oldest `vrclog-*.db` files beyond `keep` (names sort chronologically). Returns the number deleted.
pub fn prune_backups(dir: &Path, keep: usize) -> io::Result<usize> {
    let mut names: Vec<String> = std::fs::read_dir(dir)?
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| is_backup_name(n))
        .collect();
    names.sort();
    let excess = names.len().saturating_sub(keep);
    for n in &names[..excess] {
        std::fs::remove_file(dir.join(n))?;
    }
    Ok(excess)
}

/// Starts the idle-maintenance loop (call once from setup).
pub fn start(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(CHECK_INTERVAL).await;
            if app.state::<AppState>().importing.load(Ordering::SeqCst) {
                continue;
            }
            // VR controllers do not count as keyboard/mouse input, so a VR session looks idle:
            // never run maintenance while VRChat itself is running.
            let idle_enough = tauri::async_runtime::spawn_blocking(|| {
                idle::idle_seconds() >= IDLE_REQUIRED_SECS
                    && !idle::process_running("VRChat.exe")
                    && idle::cpu_busy_fraction(CPU_SAMPLE) < CPU_BUSY_LIMIT
            })
            .await
            .unwrap_or(false);
            if !idle_enough {
                continue;
            }
            let (auto_vacuum, min_mb, last, interval) = {
                let s = app.state::<AppState>().settings.lock().unwrap().clone();
                (s.auto_vacuum, s.auto_vacuum_min_db_mb, s.last_backup_at, s.backup_interval_days)
            };
            let (size, reclaimable) = app::db_sizes(&app);
            if auto_vacuum && should_vacuum(size, reclaimable, min_mb) {
                app::vacuum(app.clone()).await;
            } else if backup_due(last.as_deref(), Local::now().naive_local(), interval) {
                backup_now(app.clone(), true).await;
            }
        }
    });
}

pub async fn backup_now(app: AppHandle, automatic: bool) {
    if !app::try_begin(&app) {
        return;
    }
    let (db_path, folder, keep) = {
        let s = app.state::<AppState>().settings.lock().unwrap().clone();
        (s.resolved_db_path(), s.resolved_backup_folder(), s.backup_keep_count.max(1) as usize)
    };
    if !automatic {
        app::set_status(&app, "バックアップ中…");
    }
    let r = tauri::async_runtime::spawn_blocking(move || -> Result<String, String> {
        let conn = app::open_db(&db_path).map_err(|e| user_message(&e))?;
        std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
        let dest = folder.join(backup_file_name(Local::now().naive_local()));
        db::backup_into(&conn, &dest).map_err(|e| user_message(&e))?;
        // A failed prune must not turn a finished backup into an error.
        let _ = prune_backups(&folder, keep);
        Ok(dest.display().to_string())
    })
    .await;
    match app::flatten_join(r) {
        Ok(path) => {
            let saved = {
                let st = app.state::<AppState>();
                let mut s = st.settings.lock().unwrap();
                s.last_backup_at = Some(Local::now().format(TS_FORMAT).to_string());
                let _ = s.save();
                s.clone()
            };
            let _ = app.emit("settings-changed", saved);
            app::set_status(&app, format!("バックアップしました: {path}"));
        }
        Err(msg) => app::set_status(&app, format!("バックアップエラー: {msg}")),
    }
    app::release_importing(&app);
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    const MIB: i64 = 1024 * 1024;

    fn dt(d: u32, h: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 5, d).unwrap().and_hms_opt(h, 0, 0).unwrap()
    }

    #[test]
    fn vacuum_thresholds() {
        assert!(!should_vacuum(199 * MIB, 100 * MIB, 200));
        assert!(should_vacuum(200 * MIB, 40 * MIB, 200)); // exactly 20 %
        assert!(!should_vacuum(200 * MIB, 40 * MIB - 1, 200));
        assert!(!should_vacuum(0, 0, 0));
        assert!(!should_vacuum(0, 10, 0));
    }

    #[test]
    fn backup_due_rules() {
        let now = dt(10, 12);
        assert!(!backup_due(None, now, 0));
        assert!(!backup_due(Some("2000-01-01 00:00:00"), now, 0));
        assert!(backup_due(None, now, 7));
        assert!(backup_due(Some("garbage"), now, 7));
        assert!(!backup_due(Some("2026-05-03 12:00:01"), now, 7)); // just under
        assert!(backup_due(Some("2026-05-03 12:00:00"), now, 7)); // exactly
        assert!(backup_due(Some("2026-05-03 11:59:59"), now, 7));
    }

    #[test]
    fn file_name_format() {
        let t = NaiveDate::from_ymd_opt(2026, 5, 7).unwrap().and_hms_opt(8, 9, 10).unwrap();
        assert_eq!(backup_file_name(t), "vrclog-20260507-080910.db");
    }

    #[test]
    fn prune_keeps_newest_and_ignores_others() {
        let dir = tempfile::tempdir().unwrap();
        for n in [
            "vrclog-20260101-000000.db",
            "vrclog-20260102-000000.db",
            "vrclog-20260103-000000.db",
            "vrclog-20260104-000000.db",
            "vrclog.db",
            "notes.txt",
            "vrclog-20250101-000000.db.bak",
        ] {
            std::fs::write(dir.path().join(n), b"x").unwrap();
        }
        assert_eq!(prune_backups(dir.path(), 2).unwrap(), 2);
        let mut left: Vec<String> =
            std::fs::read_dir(dir.path()).unwrap().map(|e| e.unwrap().file_name().into_string().unwrap()).collect();
        left.sort();
        assert_eq!(
            left,
            [
                "notes.txt",
                "vrclog-20250101-000000.db.bak",
                "vrclog-20260103-000000.db",
                "vrclog-20260104-000000.db",
                "vrclog.db"
            ]
        );
        assert_eq!(prune_backups(dir.path(), 5).unwrap(), 0);
    }
}
