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
use vrclogger_core::data::metadata::{MetaOutcome, MetaPlan, MetaTarget, MetadataService};

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
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct MetaProgress {
    done: usize,
    total: usize,
    file_name: String,
}

/// Status line shown after a run.
fn summary_status(mode: Mode, s: &MetaRunSummary) -> String {
    let verb = match mode {
        Mode::Write => "書き込みました",
        Mode::Remove => "取り除きました",
    };
    let cancelled = if s.cancelled { "（中止しました）" } else { "" };
    format!("メタデータを{verb}: {} 枚（対象外 {} / 失敗 {}）{cancelled}", s.done, s.skipped, s.failed)
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

fn db_inputs(state: &AppState) -> (std::path::PathBuf, u32) {
    let s = state.settings.lock().unwrap();
    (s.resolved_db_path(), s.day_start_hour)
}

#[tauri::command]
pub async fn metadata_plan(app: AppHandle, state: State<'_, AppState>, target: MetaTarget, mode: String) -> CmdResult<MetaPlan> {
    let mode = Mode::parse(&mode)?;
    if state.importing.load(Ordering::SeqCst) {
        return Err(BUSY_PLAN.to_string());
    }
    let (db_path, hour) = db_inputs(&state);
    let previous = state.status.lock().unwrap().clone();
    app::set_status(&app, "対象を確認中…");
    let r = tauri::async_runtime::spawn_blocking(move || -> Result<MetaPlan, String> {
        let conn = app::open_db(&db_path).map_err(|e| e.to_string())?;
        let svc = MetadataService::new(&conn);
        let ids = svc.resolve(&target, hour).map_err(|e| e.to_string())?;
        make_plan(&svc, &ids, mode, Local::now().naive_local()).map_err(|e| e.to_string())
    })
    .await;
    app::set_status(&app, previous);
    r.map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn metadata_cancel(state: State<'_, AppState>) {
    state.metadata_cancel.store(true, Ordering::SeqCst);
}

#[tauri::command]
pub async fn metadata_run(app: AppHandle, state: State<'_, AppState>, target: MetaTarget, mode: String) -> CmdResult<MetaRunSummary> {
    let mode = Mode::parse(&mode)?;
    if !app::try_begin(&app) {
        return Err(BUSY_PLAN.to_string());
    }
    state.metadata_cancel.store(false, Ordering::SeqCst);
    let (db_path, hour) = db_inputs(&state);
    app::set_status(&app, match mode {
        Mode::Write => "メタデータを書き込み中…",
        Mode::Remove => "メタデータを取り除き中…",
    });
    let handle = app.clone();
    let r = tauri::async_runtime::spawn_blocking(move || run_blocking(&handle, &db_path, hour, &target, mode)).await;
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

fn run_blocking(app: &AppHandle, db_path: &std::path::Path, hour: u32, target: &MetaTarget, mode: Mode) -> Result<MetaRunSummary, String> {
    let conn = app::open_db(db_path).map_err(|e| e.to_string())?;
    let svc = MetadataService::new(&conn);
    let ids = svc.resolve(target, hour).map_err(|e| e.to_string())?;
    let now = Local::now().naive_local();
    let plan = make_plan(&svc, &ids, mode, now).map_err(|e| e.to_string())?;
    let offset = local_offset_minutes();
    let total = plan.eligible;
    let cancel = &app.state::<AppState>().metadata_cancel;

    let mut sum = MetaRunSummary { skipped: plan.total - plan.eligible, ..Default::default() };
    let mut last_emit: Option<Instant> = None;
    for (i, &id) in plan.eligible_ids.iter().enumerate() {
        if cancel.load(Ordering::SeqCst) {
            sum.cancelled = true;
            break;
        }
        let file_name: String = conn
            .query_row("SELECT file_name FROM photos WHERE id = ?1", [id], |r| r.get(0))
            .optional()
            .map_err(|e| e.to_string())?
            .unwrap_or_default();
        if last_emit.is_none_or(|t| t.elapsed() >= PROGRESS_INTERVAL) {
            last_emit = Some(Instant::now());
            let _ = app.emit("metadata-progress", MetaProgress { done: i, total, file_name: file_name.clone() });
        }
        let outcome = match mode {
            Mode::Write => svc.apply_write(id, offset, now),
            Mode::Remove => svc.apply_remove(id),
        }
        .unwrap_or_else(|e| MetaOutcome::Failed(e.to_string()));
        match outcome {
            MetaOutcome::Done => sum.done += 1,
            MetaOutcome::Skipped(_) => sum.skipped += 1,
            MetaOutcome::Failed(message) => {
                sum.failed += 1;
                if sum.failures.len() < MAX_FAILURES {
                    sum.failures.push(MetaFailure { file_name, message });
                }
            }
        }
    }
    let _ = app.emit(
        "metadata-progress",
        MetaProgress { done: sum.done + sum.failed + (sum.skipped - (plan.total - plan.eligible)), total, file_name: String::new() },
    );
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
