//! Tauri commands for writing log-derived metadata into photos (and removing it again).
//!
//! Every write goes plan -> user confirmation (UI) -> run. Per-file safety lives in the core
//! `MetadataService`; this module only orchestrates, reports progress and honours cancel.

use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use chrono::{Local, NaiveDateTime};
use rusqlite::OptionalExtension;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use vrclogger_core::data::metadata::{DownloadOutcome, MetaOutcome, MetaPlan, MetaStatus, MetaTarget, MetadataService};
use vrclogger_core::photos::cloud;

use crate::app::{self, AppState};
use crate::commands::CmdResult;
use crate::photos::{self, PhotosChanged};

const BUSY_PLAN: &str = "取り込み中は実行できません。";
const MAX_FAILURES: usize = 20;
const PROGRESS_INTERVAL: Duration = Duration::from_millis(100);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Write,
    Remove,
}

impl Mode {
    fn parse(s: &str) -> CmdResult<Self> {
        match s {
            "write" => Ok(Mode::Write),
            "remove" => Ok(Mode::Remove),
            other => Err(format!("不明な操作です: {other}")),
        }
    }
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MetaFailure {
    pub file_name: String,
    pub message: String,
}

#[derive(Serialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MetaRunSummary {
    pub done: usize,
    pub skipped: usize,
    pub failed: usize,
    pub cancelled: bool,
    pub failures: Vec<MetaFailure>,
    /// Online-only files that were downloaded to be checked (they stay on this PC).
    pub downloaded: usize,
    pub downloaded_bytes: i64,
    /// Downloading stopped early: too little free disk space.
    pub low_disk: bool,
    /// Downloading stopped early: several downloads in a row failed.
    pub download_stopped: bool,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct MetaProgress {
    done: usize,
    total: usize,
    file_name: String,
    downloading: bool,
}

/// Free space kept on the photo drive while downloading (besides the file itself).
const DISK_RESERVE: u64 = 2 << 30;
/// Downloading stops after this many failures in a row (sync client stopped, offline, blocked).
const MAX_DOWNLOAD_FAILURES_IN_A_ROW: usize = 5;

/// Status line shown after a run.
fn summary_status(mode: Mode, s: &MetaRunSummary) -> String {
    let verb = match mode {
        Mode::Write => "書き込みました",
        Mode::Remove => "取り除きました",
    };
    let cancelled = if s.cancelled { "（中止しました）" } else { "" };
    format!("メタデータを{verb}: {} 枚（対象外 {} / 失敗 {}）{cancelled}", s.done, s.skipped, s.failed)
}

impl MetaRunSummary {
    fn count(&mut self, outcome: MetaOutcome, file_name: String) {
        match outcome {
            MetaOutcome::Done => self.done += 1,
            MetaOutcome::Skipped(_) => self.skipped += 1,
            MetaOutcome::Failed(message) => {
                self.failed += 1;
                if self.failures.len() < MAX_FAILURES {
                    self.failures.push(MetaFailure { file_name, message });
                }
            }
        }
    }
}

/// The UTC offset of the machine's local zone, in minutes. Core takes one offset per call and
/// only ids are known here, so the current offset is used for every photo (Japan has no DST).
fn local_offset_minutes() -> i32 {
    Local::now().offset().local_minus_utc() / 60
}

fn make_plan(svc: &MetadataService, ids: &[i64], mode: Mode, now: NaiveDateTime) -> vrclogger_core::Result<MetaPlan> {
    match mode {
        Mode::Write => svc.plan_write(ids, local_offset_minutes(), now),
        Mode::Remove => svc.plan_remove(ids),
    }
}

fn db_path(state: &AppState) -> std::path::PathBuf {
    state.settings.lock().unwrap().resolved_db_path()
}

#[tauri::command]
pub async fn metadata_plan(app: AppHandle, state: State<'_, AppState>, target: MetaTarget, mode: String) -> CmdResult<MetaPlan> {
    let mode = Mode::parse(&mode)?;
    if state.importing.load(Ordering::SeqCst) {
        return Err(BUSY_PLAN.to_string());
    }
    let db_path = db_path(&state);
    let previous = state.status.lock().unwrap().clone();
    app::set_status(&app, "対象を確認中…");
    let r = tauri::async_runtime::spawn_blocking(move || -> Result<MetaPlan, String> {
        let conn = app::open_db(&db_path).map_err(|e| e.to_string())?;
        let svc = MetadataService::new(&conn);
        let ids = svc.resolve(&target).map_err(|e| e.to_string())?;
        make_plan(&svc, &ids, mode, Local::now().naive_local()).map_err(|e| e.to_string())
    })
    .await;
    app::set_status(&app, previous);
    r.map_err(|e| e.to_string())?
}

/// Metadata state of one photo (context menu / info dialog). Reads at most a local file's header.
#[tauri::command]
pub async fn photo_meta_status(state: State<'_, AppState>, id: i64) -> CmdResult<MetaStatus> {
    let db_path = db_path(&state);
    tauri::async_runtime::spawn_blocking(move || -> Result<MetaStatus, String> {
        let conn = app::open_db(&db_path).map_err(|e| e.to_string())?;
        MetadataService::new(&conn)
            .status(id, local_offset_minutes(), Local::now().naive_local())
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn metadata_cancel(state: State<'_, AppState>) {
    state.metadata_cancel.store(true, Ordering::SeqCst);
}

/// `allow_download`: also process online-only candidates (write only) by downloading them first.
/// Downloaded files stay on this PC.
#[tauri::command]
pub async fn metadata_run(
    app: AppHandle,
    state: State<'_, AppState>,
    target: MetaTarget,
    mode: String,
    allow_download: Option<bool>,
) -> CmdResult<MetaRunSummary> {
    let mode = Mode::parse(&mode)?;
    let allow_download = allow_download.unwrap_or(false) && mode == Mode::Write;
    if !app::try_begin(&app) {
        return Err(BUSY_PLAN.to_string());
    }
    state.metadata_cancel.store(false, Ordering::SeqCst);
    let db_path = db_path(&state);
    app::set_status(&app, match mode {
        Mode::Write => "メタデータを書き込み中…",
        Mode::Remove => "メタデータを取り除き中…",
    });
    let handle = app.clone();
    let r = tauri::async_runtime::spawn_blocking(move || run_blocking(&handle, &db_path, &target, mode, allow_download))
        .await;
    app::release_importing(&app);
    match r {
        Ok(Ok(sum)) => {
            photos::emit_changed(&app, &PhotosChanged { updated: sum.done, ..Default::default() });
            app::set_status(&app, summary_status(mode, &sum));
            Ok(sum)
        }
        Ok(Err(msg)) => {
            app::set_status(&app, format!("メタデータのエラー: {msg}"));
            Err(msg)
        }
        Err(e) => {
            app::set_status(&app, format!("メタデータのエラー: {e}"));
            Err(e.to_string())
        }
    }
}

fn file_row(conn: &rusqlite::Connection, id: i64) -> Result<(String, String, i64), String> {
    conn.query_row("SELECT file_name, path, file_size FROM photos WHERE id = ?1", [id], |r| {
        Ok((r.get(0)?, r.get(1)?, r.get(2)?))
    })
    .optional()
    .map_err(|e| e.to_string())
    .map(Option::unwrap_or_default)
}

fn run_blocking(
    app: &AppHandle,
    db_path: &std::path::Path,
    target: &MetaTarget,
    mode: Mode,
    allow_download: bool,
) -> Result<MetaRunSummary, String> {
    let conn = app::open_db(db_path).map_err(|e| e.to_string())?;
    let svc = MetadataService::new(&conn);
    let ids = svc.resolve(target).map_err(|e| e.to_string())?;
    let now = Local::now().naive_local();
    let plan = make_plan(&svc, &ids, mode, now).map_err(|e| e.to_string())?;
    let offset = local_offset_minutes();
    let downloads: &[i64] = if allow_download { &plan.download_ids } else { &[] };
    let total = plan.eligible + downloads.len();
    let cancel = &app.state::<AppState>().metadata_cancel;

    let mut sum = MetaRunSummary { skipped: plan.total - plan.eligible - downloads.len(), ..Default::default() };
    let initially_skipped = sum.skipped;
    let mut last_emit: Option<Instant> = None;
    let mut emit = |done: usize, file_name: &str, downloading: bool, force: bool| {
        if force || downloading || last_emit.is_none_or(|t| t.elapsed() >= PROGRESS_INTERVAL) {
            last_emit = Some(Instant::now());
            let _ = app.emit("metadata-progress", MetaProgress { done, total, file_name: file_name.to_string(), downloading });
        }
    };

    for (i, &id) in plan.eligible_ids.iter().enumerate() {
        if cancel.load(Ordering::SeqCst) {
            sum.cancelled = true;
            break;
        }
        let (file_name, _, _) = file_row(&conn, id)?;
        emit(i, &file_name, false, false);
        let outcome = match mode {
            Mode::Write => svc.apply_write(id, offset, now),
            Mode::Remove => svc.apply_remove(id),
        }
        .unwrap_or_else(|e| MetaOutcome::Failed(e.to_string()));
        sum.count(outcome, file_name);
    }

    // Online-only candidates, one at a time: download, check, write. They stay local afterwards.
    let mut failures_in_a_row = 0;
    for (i, &id) in downloads.iter().enumerate() {
        if sum.cancelled || cancel.load(Ordering::SeqCst) {
            sum.cancelled = true;
            break;
        }
        let (file_name, path, size) = file_row(&conn, id)?;
        let free = cloud::available_bytes(std::path::Path::new(&path));
        if free.is_some_and(|f| f < size.max(0) as u64 + DISK_RESERVE) {
            sum.low_disk = true;
            break;
        }
        emit(plan.eligible + i, &file_name, true, false);
        let r = svc
            .apply_write_downloading(id, offset, now)
            .unwrap_or_else(|e| DownloadOutcome { outcome: MetaOutcome::Failed(e.to_string()), downloaded: false });
        if r.downloaded {
            sum.downloaded += 1;
            sum.downloaded_bytes += size.max(0);
        }
        failures_in_a_row = if matches!(r.outcome, MetaOutcome::Failed(_)) { failures_in_a_row + 1 } else { 0 };
        sum.count(r.outcome, file_name);
        if failures_in_a_row >= MAX_DOWNLOAD_FAILURES_IN_A_ROW {
            sum.download_stopped = true;
            break;
        }
    }
    emit(sum.done + sum.failed + (sum.skipped - initially_skipped), "", false, true);
    Ok(sum)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_parses_known_values_only() {
        assert_eq!(Mode::parse("write"), Ok(Mode::Write));
        assert_eq!(Mode::parse("remove"), Ok(Mode::Remove));
        assert!(Mode::parse("x").is_err());
    }

    #[test]
    fn status_text() {
        let s = MetaRunSummary { done: 3, skipped: 1, failed: 2, ..Default::default() };
        assert_eq!(summary_status(Mode::Write, &s), "メタデータを書き込みました: 3 枚（対象外 1 / 失敗 2）");
        let c = MetaRunSummary { cancelled: true, ..s };
        assert_eq!(summary_status(Mode::Remove, &c), "メタデータを取り除きました: 3 枚（対象外 1 / 失敗 2）（中止しました）");
    }
}
