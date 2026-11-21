//! Running a [`MetaPlan`]: the batch loop behind "write metadata into all photos".
//!
//! Local candidates are processed first, then online-only ones one at a time (download, check,
//! write), stopping early when the disk gets too full or downloads keep failing. Everything
//! outside the loop is injected through [`RunHooks`] (cancel flag, free disk space, progress,
//! wording of failures) and [`RunSteps`] (what happens to one photo), so the loop itself needs
//! neither the UI nor real cloud files.

use std::path::Path;
use std::time::{Duration, Instant};

use chrono::NaiveDateTime;
use serde::Serialize;

use super::metadata::{DownloadOutcome, FileRow, MetaMode, MetaOutcome, MetaPlan, MetadataService};
use crate::{PhotoError, Result};

/// At most this many failures are listed in a summary (all of them are counted).
pub const MAX_FAILURES: usize = 20;
/// Free space kept on the photo drive while downloading (besides the file itself).
pub const DISK_RESERVE: u64 = 2 << 30;
/// Downloading stops after this many failures in a row (sync client stopped, offline, blocked).
pub const MAX_DOWNLOAD_FAILURES_IN_A_ROW: usize = 5;
/// Progress is reported at most this often (downloads and the final report always are).
pub const PROGRESS_INTERVAL: Duration = Duration::from_millis(100);

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

#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MetaProgress {
    pub done: usize,
    pub total: usize,
    pub file_name: String,
    pub downloading: bool,
}

impl MetaRunSummary {
    fn count(&mut self, outcome: MetaOutcome, file_name: String, explain: &dyn Fn(&PhotoError) -> String) {
        match outcome {
            MetaOutcome::Done => self.done += 1,
            MetaOutcome::Skipped(_) => self.skipped += 1,
            MetaOutcome::Failed(cause) => {
                self.failed += 1;
                if self.failures.len() < MAX_FAILURES {
                    self.failures.push(MetaFailure { file_name, message: explain(&cause) });
                }
            }
        }
    }
}

/// What happens to one photo. [`ServiceSteps`] is the real thing; tests substitute their own.
pub trait RunSteps {
    /// File name, path and size of a photo (empty values for an unknown id).
    fn file_row(&self, id: i64) -> Result<FileRow>;
    /// Write or remove metadata for a local photo.
    fn apply(&self, mode: MetaMode, id: i64) -> Result<MetaOutcome>;
    /// Write metadata, downloading the file first if it is online-only.
    fn apply_downloading(&self, id: i64) -> Result<DownloadOutcome>;
}

/// [`RunSteps`] backed by a [`MetadataService`].
pub struct ServiceSteps<'a> {
    pub svc: &'a MetadataService<'a>,
    pub utc_offset_minutes: i32,
    pub now: NaiveDateTime,
}

impl RunSteps for ServiceSteps<'_> {
    fn file_row(&self, id: i64) -> Result<FileRow> {
        Ok(self.svc.file_row(id)?.unwrap_or_default())
    }

    fn apply(&self, mode: MetaMode, id: i64) -> Result<MetaOutcome> {
        match mode {
            MetaMode::Write => self.svc.apply_write(id, self.utc_offset_minutes, self.now),
            MetaMode::Remove => self.svc.apply_remove(id),
        }
    }

    fn apply_downloading(&self, id: i64) -> Result<DownloadOutcome> {
        self.svc.apply_write_downloading(id, self.utc_offset_minutes, self.now)
    }
}

/// Everything the loop needs from its surroundings.
pub struct RunHooks<'a> {
    /// Polled before each file; true stops the run after the current file.
    pub is_cancelled: &'a dyn Fn() -> bool,
    /// Free bytes on the volume of a path (`None` if unknown: downloading goes on).
    pub free_bytes: &'a dyn Fn(&Path) -> Option<u64>,
    /// Receives progress reports (already thinned out, see [`RunOptions::progress_interval`]).
    pub on_progress: &'a mut dyn FnMut(&MetaProgress),
    /// Words a failure for the user's failure list.
    pub explain: &'a dyn Fn(&PhotoError) -> String,
}

#[derive(Debug, Clone)]
pub struct RunOptions {
    /// Also process online-only candidates (downloads them). Only meaningful for a write.
    pub allow_download: bool,
    pub progress_interval: Duration,
    pub disk_reserve: u64,
    pub max_download_failures_in_a_row: usize,
}

impl RunOptions {
    pub fn new(allow_download: bool) -> Self {
        Self {
            allow_download,
            progress_interval: PROGRESS_INTERVAL,
            disk_reserve: DISK_RESERVE,
            max_download_failures_in_a_row: MAX_DOWNLOAD_FAILURES_IN_A_ROW,
        }
    }
}

/// Reports progress at most once per interval, except downloads and forced reports.
struct ProgressGate {
    interval: Duration,
    last: Option<Instant>,
}

impl ProgressGate {
    fn emit(&mut self, hooks: &mut RunHooks, p: MetaProgress, force: bool) {
        if force || p.downloading || self.last.is_none_or(|t| t.elapsed() >= self.interval) {
            self.last = Some(Instant::now());
            (hooks.on_progress)(&p);
        }
    }
}

/// Runs `plan`: its eligible photos, then (with `allow_download`) its online-only candidates.
/// Only database errors end the run with `Err`; file problems are counted as failures.
pub fn run_plan(
    steps: &dyn RunSteps,
    plan: &MetaPlan,
    mode: MetaMode,
    opts: &RunOptions,
    hooks: &mut RunHooks,
) -> Result<MetaRunSummary> {
    let downloads: &[i64] = if opts.allow_download && mode == MetaMode::Write { &plan.download_ids } else { &[] };
    let total = plan.eligible + downloads.len();

    let mut sum = MetaRunSummary { skipped: plan.total - plan.eligible - downloads.len(), ..Default::default() };
    let initially_skipped = sum.skipped;
    let mut gate = ProgressGate { interval: opts.progress_interval, last: None };
    let mut report = |hooks: &mut RunHooks, done: usize, file_name: &str, downloading: bool, force: bool| {
        gate.emit(hooks, MetaProgress { done, total, file_name: file_name.to_string(), downloading }, force);
    };

    for (i, &id) in plan.eligible_ids.iter().enumerate() {
        if (hooks.is_cancelled)() {
            sum.cancelled = true;
            break;
        }
        let FileRow { file_name, .. } = steps.file_row(id)?;
        report(hooks, i, &file_name, false, false);
        let outcome = steps.apply(mode, id).unwrap_or_else(|e| MetaOutcome::Failed(e.into()));
        sum.count(outcome, file_name, hooks.explain);
    }

    // Online-only candidates, one at a time: download, check, write. They stay local afterwards.
    let mut failures_in_a_row = 0;
    for (i, &id) in downloads.iter().enumerate() {
        if sum.cancelled || (hooks.is_cancelled)() {
            sum.cancelled = true;
            break;
        }
        let FileRow { file_name, path, file_size: size } = steps.file_row(id)?;
        let free = (hooks.free_bytes)(Path::new(&path));
        if free.is_some_and(|f| f < size.max(0) as u64 + opts.disk_reserve) {
            sum.low_disk = true;
            break;
        }
        report(hooks, plan.eligible + i, &file_name, true, false);
        let r = steps
            .apply_downloading(id)
            .unwrap_or_else(|e| DownloadOutcome { outcome: MetaOutcome::Failed(e.into()), downloaded: false });
        if r.downloaded {
            sum.downloaded += 1;
            sum.downloaded_bytes += size.max(0);
        }
        failures_in_a_row = if matches!(r.outcome, MetaOutcome::Failed(_)) { failures_in_a_row + 1 } else { 0 };
        sum.count(r.outcome, file_name, hooks.explain);
        if failures_in_a_row >= opts.max_download_failures_in_a_row {
            sum.download_stopped = true;
            break;
        }
    }
    report(hooks, sum.done + sum.failed + (sum.skipped - initially_skipped), "", false, true);
    Ok(sum)
}
