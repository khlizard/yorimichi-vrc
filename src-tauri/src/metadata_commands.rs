//! Tauri commands for writing log-derived metadata into photos (and removing it again).
//!
//! Every write goes plan -> user confirmation (UI) -> run. Per-file safety lives in the core
//! `MetadataService`, the batch loop in `yorimichi_core::data::metadata_run`; this module only
//! wires them to the app: progress events, the cancel flag and the wording of errors.

use std::sync::atomic::Ordering;

use chrono::{Local, LocalResult, NaiveDateTime, TimeZone};
use tauri::{AppHandle, Emitter, Manager, State};
use yorimichi_core::data::metadata::{MetaMode, MetaPlan, MetaStatus, MetaTarget, MetadataService};
use yorimichi_core::data::metadata_run::{run_plan, MetaProgress, MetaRunSummary, RunHooks, RunOptions, ServiceSteps};
use yorimichi_core::photos::cloud;

use crate::app::{self, AppState};
use crate::commands::CmdResult;
use crate::errors::{photo_message, user_message};
use crate::photos::{self, PhotosChanged};

const BUSY_PLAN: &str = "取り込み中は実行できません。";

fn parse_mode(s: &str) -> CmdResult<MetaMode> {
    match s {
        "write" => Ok(MetaMode::Write),
        "remove" => Ok(MetaMode::Remove),
        other => Err(format!("不明な操作です: {other}")),
    }
}

/// Status line shown after a run.
fn summary_status(mode: MetaMode, s: &MetaRunSummary) -> String {
    let verb = match mode {
        MetaMode::Write => "書き込みました",
        MetaMode::Remove => "取り除きました",
    };
    let cancelled = if s.cancelled { "（中止しました）" } else { "" };
    format!("メタデータを{verb}: {} 枚（対象外 {} / 失敗 {}）{cancelled}", s.done, s.skipped, s.failed)
}

/// The UTC offset of the machine's local zone right now, in minutes (dates the time of writing).
fn local_offset_minutes() -> i32 {
    Local::now().offset().local_minus_utc() / 60
}

/// The UTC offset that applied at a local wall-clock time, in minutes. Differs from the current
/// one across a daylight-saving change. A time skipped by the change falls back to the current offset.
fn local_offset_at(t: NaiveDateTime) -> i32 {
    match Local.from_local_datetime(&t) {
        LocalResult::Single(d) | LocalResult::Ambiguous(d, _) => d.offset().local_minus_utc() / 60,
        LocalResult::None => local_offset_minutes(),
    }
}

fn service(conn: &rusqlite::Connection) -> MetadataService<'_> {
    MetadataService::new(conn).with_capture_offset(local_offset_at)
}

fn db_path(state: &AppState) -> std::path::PathBuf {
    state.settings.lock().unwrap().resolved_db_path()
}

#[tauri::command]
pub async fn metadata_plan(app: AppHandle, state: State<'_, AppState>, target: MetaTarget, mode: String) -> CmdResult<MetaPlan> {
    let mode = parse_mode(&mode)?;
    if state.importing.load(Ordering::SeqCst) {
        return Err(BUSY_PLAN.to_string());
    }
    let db_path = db_path(&state);
    const CHECKING: &str = "対象を確認中…";
    let previous = state.status.lock().unwrap().clone();
    app::set_status(&app, CHECKING);
    let r = tauri::async_runtime::spawn_blocking(move || -> Result<MetaPlan, String> {
        let conn = app::open_db(&db_path).map_err(|e| user_message(&e))?;
        let svc = service(&conn);
        let ids = svc.resolve(&target).map_err(|e| user_message(&e))?;
        svc.plan(&ids, mode, local_offset_minutes(), Local::now().naive_local()).map_err(|e| user_message(&e))
    })
    .await;
    // Put the earlier status back, unless something else reported in the meantime.
    if *state.status.lock().unwrap() == CHECKING {
        app::set_status(&app, previous);
    }
    app::flatten_join(r)
}

/// Metadata state of one photo (context menu / info dialog). Reads at most a local file's header.
#[tauri::command]
pub async fn photo_meta_status(state: State<'_, AppState>, id: i64) -> CmdResult<MetaStatus> {
    // The connection kept for photo lookups: no new connection (and schema check) per right click.
    app::with_photo_conn(&state, |conn| service(conn).status(id, local_offset_minutes(), Local::now().naive_local()))
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
    let mode = parse_mode(&mode)?;
    let allow_download = allow_download.unwrap_or(false) && mode == MetaMode::Write;
    if !app::try_begin(&app) {
        return Err(BUSY_PLAN.to_string());
    }
    state.metadata_cancel.store(false, Ordering::SeqCst);
    let db_path = db_path(&state);
    app::set_status(&app, match mode {
        MetaMode::Write => "メタデータを書き込み中…",
        MetaMode::Remove => "メタデータを取り除き中…",
    });
    let handle = app.clone();
    let r = tauri::async_runtime::spawn_blocking(move || run_blocking(&handle, &db_path, &target, mode, allow_download))
        .await;
    app::release_importing(&app);
    match app::flatten_join(r) {
        Ok(sum) => {
            photos::emit_changed(&app, &PhotosChanged { updated: sum.done, ..Default::default() });
            app::set_status(&app, summary_status(mode, &sum));
            Ok(sum)
        }
        Err(msg) => {
            app::set_status(&app, format!("メタデータのエラー: {msg}"));
            Err(msg)
        }
    }
}

/// Plans and runs on a connection of its own; progress goes out as `metadata-progress` events.
fn run_blocking(
    app: &AppHandle,
    db_path: &std::path::Path,
    target: &MetaTarget,
    mode: MetaMode,
    allow_download: bool,
) -> Result<MetaRunSummary, String> {
    let conn = app::open_db(db_path).map_err(|e| user_message(&e))?;
    let svc = service(&conn);
    let ids = svc.resolve(target).map_err(|e| user_message(&e))?;
    let now = Local::now().naive_local();
    let offset = local_offset_minutes();
    let plan = svc.plan(&ids, mode, offset, now).map_err(|e| user_message(&e))?;
    let steps = ServiceSteps { svc: &svc, utc_offset_minutes: offset, now };

    let cancel = &app.state::<AppState>().metadata_cancel;
    let mut emit = |p: &MetaProgress| {
        let _ = app.emit("metadata-progress", p.clone());
    };
    let mut hooks = RunHooks {
        is_cancelled: &|| cancel.load(Ordering::SeqCst),
        free_bytes: &cloud::available_bytes,
        on_progress: &mut emit,
        explain: &photo_message,
    };
    run_plan(&steps, &plan, mode, &RunOptions::new(allow_download), &mut hooks).map_err(|e| user_message(&e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_parses_known_values_only() {
        assert_eq!(parse_mode("write"), Ok(MetaMode::Write));
        assert_eq!(parse_mode("remove"), Ok(MetaMode::Remove));
        assert!(parse_mode("x").is_err());
    }

    #[test]
    fn status_text() {
        let s = MetaRunSummary { done: 3, skipped: 1, failed: 2, ..Default::default() };
        assert_eq!(summary_status(MetaMode::Write, &s), "メタデータを書き込みました: 3 枚（対象外 1 / 失敗 2）");
        let c = MetaRunSummary { cancelled: true, ..s };
        assert_eq!(summary_status(MetaMode::Remove, &c), "メタデータを取り除きました: 3 枚（対象外 1 / 失敗 2）（中止しました）");
    }
}
