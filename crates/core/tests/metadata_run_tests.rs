mod common;
#[path = "common/xmp_helpers.rs"]
mod xmp_helpers;

use std::cell::{Cell, RefCell};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use common::{dt, import_visits, Fixture, V};
use vrclogger_core::data::metadata::{
    DownloadOutcome, FileRow, MetaMode, MetaOutcome, MetaPlan, MetaTarget, MetadataService, SkipReason,
};
use vrclogger_core::data::metadata_run::{
    run_plan, MetaProgress, RunHooks, RunOptions, RunSteps, ServiceSteps, DISK_RESERVE, MAX_FAILURES,
};
use vrclogger_core::data::PhotoService;
use vrclogger_core::photos::{scan_folders, xmp_presence, XmpPresence};
use vrclogger_core::{PhotoError, Result};
use xmp_helpers::*;

/// Steps with scripted outcomes; remembers which ids were processed.
struct Fake<'a> {
    local: &'a dyn Fn(i64) -> MetaOutcome,
    download: &'a dyn Fn(i64) -> DownloadOutcome,
    size: i64,
    applied: RefCell<Vec<i64>>,
    downloaded: RefCell<Vec<i64>>,
}

impl RunSteps for Fake<'_> {
    fn file_row(&self, id: i64) -> Result<FileRow> {
        Ok(FileRow { file_name: format!("f{id}.png"), path: format!("C:\\p\\f{id}.png"), file_size: self.size })
    }
    fn apply(&self, _mode: MetaMode, id: i64) -> Result<MetaOutcome> {
        self.applied.borrow_mut().push(id);
        Ok((self.local)(id))
    }
    fn apply_downloading(&self, id: i64) -> Result<DownloadOutcome> {
        self.downloaded.borrow_mut().push(id);
        Ok((self.download)(id))
    }
}

fn plan(total: usize, eligible: Vec<i64>, download: Vec<i64>) -> MetaPlan {
    MetaPlan {
        total,
        eligible: eligible.len(),
        eligible_ids: eligible,
        skipped: Vec::new(),
        example: None,
        online: download.len(),
        download: download.len(),
        download_ids: download,
        download_bytes: 0,
    }
}

fn ok_local(_: i64) -> MetaOutcome {
    MetaOutcome::Done
}
fn ok_download(_: i64) -> DownloadOutcome {
    DownloadOutcome { outcome: MetaOutcome::Done, downloaded: true }
}
fn failed_download(_: i64) -> DownloadOutcome {
    DownloadOutcome { outcome: MetaOutcome::Failed(PhotoError::DownloadFailed("offline".into())), downloaded: false }
}

fn fake<'a>(local: &'a dyn Fn(i64) -> MetaOutcome, download: &'a dyn Fn(i64) -> DownloadOutcome) -> Fake<'a> {
    Fake { local, download, size: 1000, applied: RefCell::default(), downloaded: RefCell::default() }
}

fn opts(allow_download: bool) -> RunOptions {
    // Every progress report goes through unless a test sets an interval.
    RunOptions { progress_interval: Duration::ZERO, ..RunOptions::new(allow_download) }
}

struct Run {
    summary: vrclogger_core::data::metadata_run::MetaRunSummary,
    progress: Vec<MetaProgress>,
}

fn run(steps: &Fake, plan: &MetaPlan, opts: &RunOptions, cancel: &dyn Fn() -> bool, free: Option<u64>) -> Run {
    let mut progress = Vec::new();
    let mut on_progress = |p: &MetaProgress| progress.push(p.clone());
    let free_bytes = move |_: &Path| free;
    let explain = |e: &PhotoError| format!("explained: {e}");
    let mut hooks = RunHooks { is_cancelled: cancel, free_bytes: &free_bytes, on_progress: &mut on_progress, explain: &explain };
    let summary = run_plan(steps, plan, MetaMode::Write, opts, &mut hooks).unwrap();
    Run { summary, progress }
}

#[test]
fn counts_done_skipped_and_failed() {
    let local = |id: i64| match id {
        3 => MetaOutcome::Skipped(SkipReason::HasMetadata),
        4 => MetaOutcome::Failed(PhotoError::ReadOnly),
        _ => MetaOutcome::Done,
    };
    let steps = fake(&local, &ok_download);
    // 7 photos in the plan, 5 eligible: 2 were skipped before the run.
    let r = run(&steps, &plan(7, vec![1, 2, 3, 4, 5], vec![]), &opts(false), &|| false, None);
    let s = r.summary;
    assert_eq!((s.done, s.skipped, s.failed), (3, 2 + 1, 1));
    assert!(!s.cancelled && !s.low_disk && !s.download_stopped);
    assert_eq!((s.downloaded, s.downloaded_bytes), (0, 0));
    assert_eq!(s.failures.len(), 1);
    assert_eq!(s.failures[0].file_name, "f4.png");
    assert_eq!(s.failures[0].message, "explained: the file is read-only");
    // The last report is the final one: everything processed in this run.
    let last = r.progress.last().unwrap();
    assert_eq!((last.done, last.total, last.file_name.as_str(), last.downloading), (5, 5, "", false));
}

#[test]
fn the_failure_list_is_capped_but_every_failure_is_counted() {
    let local = |_: i64| MetaOutcome::Failed(PhotoError::ReadOnly);
    let steps = fake(&local, &ok_download);
    let ids: Vec<i64> = (1..=(MAX_FAILURES as i64 + 5)).collect();
    let r = run(&steps, &plan(ids.len(), ids, vec![]), &opts(false), &|| false, None);
    assert_eq!(r.summary.failed, MAX_FAILURES + 5);
    assert_eq!(r.summary.failures.len(), MAX_FAILURES);
}

#[test]
fn the_cancel_flag_stops_the_run() {
    let steps = fake(&ok_local, &ok_download);
    let polls = Cell::new(0);
    let cancel = || {
        polls.set(polls.get() + 1);
        polls.get() > 2 // let two files through
    };
    let r = run(&steps, &plan(6, vec![1, 2, 3, 4], vec![10, 11]), &opts(true), &cancel, None);
    assert!(r.summary.cancelled);
    assert_eq!(r.summary.done, 2);
    assert_eq!(*steps.applied.borrow(), [1, 2]);
    assert!(steps.downloaded.borrow().is_empty());
}

#[test]
fn cancelling_during_the_local_part_also_skips_the_downloads() {
    let steps = fake(&ok_local, &ok_download);
    let r = run(&steps, &plan(3, vec![1], vec![10, 11]), &opts(true), &|| true, None);
    assert!(r.summary.cancelled);
    assert_eq!(r.summary.done, 0);
    assert!(steps.downloaded.borrow().is_empty());
}

#[test]
fn downloads_are_left_alone_unless_allowed() {
    let steps = fake(&ok_local, &ok_download);
    // 5 photos: 2 eligible, 2 online-only candidates, 1 skipped by the plan.
    let r = run(&steps, &plan(5, vec![1, 2], vec![10, 11]), &opts(false), &|| false, None);
    assert_eq!((r.summary.done, r.summary.skipped, r.summary.downloaded), (2, 3, 0));
    assert!(steps.downloaded.borrow().is_empty());
}

#[test]
fn downloads_are_counted_with_their_size() {
    let steps = fake(&ok_local, &ok_download);
    let r = run(&steps, &plan(4, vec![1], vec![10, 11, 12]), &opts(true), &|| false, Some(DISK_RESERVE + 5000));
    let s = r.summary;
    assert_eq!((s.done, s.downloaded, s.downloaded_bytes), (4, 3, 3000));
    assert!(!s.low_disk && !s.download_stopped);
    assert_eq!(*steps.downloaded.borrow(), [10, 11, 12]);
    // Progress counts local files first, then the downloads, each one reported while downloading.
    let dl: Vec<_> = r.progress.iter().filter(|p| p.downloading).map(|p| (p.done, p.total)).collect();
    assert_eq!(dl, [(1, 4), (2, 4), (3, 4)]);
}

#[test]
fn too_little_free_space_stops_the_downloads() {
    let steps = fake(&ok_local, &ok_download);
    // The file itself (1000 bytes) plus the reserve must fit.
    let r = run(&steps, &plan(4, vec![1], vec![10, 11, 12]), &opts(true), &|| false, Some(DISK_RESERVE + 999));
    let s = r.summary;
    assert!(s.low_disk);
    assert_eq!((s.done, s.downloaded), (1, 0));
    assert!(steps.downloaded.borrow().is_empty());

    // Unknown free space does not block.
    let steps = fake(&ok_local, &ok_download);
    let r = run(&steps, &plan(2, vec![], vec![10, 11]), &opts(true), &|| false, None);
    assert!(!r.summary.low_disk);
    assert_eq!(r.summary.downloaded, 2);
}

#[test]
fn repeated_download_failures_stop_the_downloads() {
    let steps = fake(&ok_local, &failed_download);
    let ids: Vec<i64> = (10..20).collect();
    let r = run(&steps, &plan(ids.len(), vec![], ids), &opts(true), &|| false, None);
    let s = r.summary;
    assert!(s.download_stopped && !s.low_disk);
    assert_eq!((s.failed, s.downloaded), (5, 0));
    assert_eq!(steps.downloaded.borrow().len(), 5);
    assert_eq!(s.failures[0].message, "explained: could not download the file: offline");
}

#[test]
fn a_success_resets_the_failure_streak() {
    let download = |id: i64| if id == 14 { ok_download(id) } else { failed_download(id) };
    let steps = fake(&ok_local, &download);
    // 4 failures, a success, 4 failures: never 5 in a row.
    let ids: Vec<i64> = (10..19).collect();
    let r = run(&steps, &plan(ids.len(), vec![], ids), &opts(true), &|| false, None);
    assert!(!r.summary.download_stopped);
    assert_eq!((r.summary.done, r.summary.failed), (1, 8));
    assert_eq!(steps.downloaded.borrow().len(), 9);
}

#[test]
fn progress_is_thinned_out_except_downloads_and_the_last_report() {
    let steps = fake(&ok_local, &ok_download);
    let slow = RunOptions { progress_interval: Duration::from_secs(3600), ..RunOptions::new(true) };
    let r = run(&steps, &plan(5, vec![1, 2, 3], vec![10, 11]), &slow, &|| false, None);
    let shown: Vec<_> = r.progress.iter().map(|p| (p.done, p.downloading, p.file_name.as_str())).collect();
    // First local file, then (throttled away) the rest, both downloads, and the final report.
    assert_eq!(shown, [(0, false, "f1.png"), (3, true, "f10.png"), (4, true, "f11.png"), (5, false, "")]);
}

// ---- with the real service on generated images ----

fn photos_dir(fx: &Fixture) -> PathBuf {
    fx.dir.path().join("photos")
}

fn put(fx: &Fixture, name: &str, bytes: &[u8]) -> PathBuf {
    let dir = photos_dir(fx);
    fs::create_dir_all(&dir).unwrap();
    let p = dir.join(name);
    fs::write(&p, bytes).unwrap();
    p
}

#[test]
fn real_run_writes_metadata_and_reports_a_broken_file() {
    let fx = Fixture::new();
    import_visits(&fx, &[V::new("2026.01.10", "20:00:00", "21:00:00", "Sample World", WRLD)]);
    let a = put(&fx, "VRChat_2026-01-10_20-30-00.000.png", &png_bytes(10, 8));
    let b = put(&fx, "VRChat_2026-01-10_20-31-00.000.jpg", &jpeg_bytes(10, 8));
    let bad = put(&fx, "VRChat_2026-01-10_20-32-00.000.png", b"this is not a png at all");
    scan_folders(&fx.conn, &[photos_dir(&fx)], None).unwrap();

    let now = dt(2026, 11, 21, 12, 0, 0);
    let svc = MetadataService::new(&fx.conn);
    let ids = svc.resolve(&MetaTarget::All).unwrap();
    let plan = svc.plan(&ids, MetaMode::Write, 540, now).unwrap();
    assert_eq!(plan.eligible, 3);

    let steps = ServiceSteps { svc: &svc, utc_offset_minutes: 540, now };
    let mut progress = Vec::new();
    let mut on_progress = |p: &MetaProgress| progress.push(p.clone());
    let explain = |e: &PhotoError| e.to_string();
    let mut hooks = RunHooks {
        is_cancelled: &|| false,
        free_bytes: &|_| None,
        on_progress: &mut on_progress,
        explain: &explain,
    };
    let s = run_plan(&steps, &plan, MetaMode::Write, &RunOptions::new(false), &mut hooks).unwrap();
    assert_eq!((s.done, s.skipped, s.failed), (2, 0, 1));
    assert_eq!(s.failures.len(), 1);
    assert_eq!(s.failures[0].file_name, "VRChat_2026-01-10_20-32-00.000.png");
    assert_eq!(xmp_presence(&a).unwrap(), XmpPresence::Ours);
    assert_eq!(xmp_presence(&b).unwrap(), XmpPresence::Ours);
    assert_eq!(fs::read(&bad).unwrap(), b"this is not a png at all");

    // Running again finds nothing left to write: the plan now skips everything but the broken file.
    let again = svc.plan(&ids, MetaMode::Write, 540, now).unwrap();
    assert_eq!(again.eligible, 1);

    // And the same loop removes it again.
    let ours = svc.plan(&ids, MetaMode::Remove, 540, now).unwrap();
    assert_eq!(ours.eligible, 2);
    let mut hooks = RunHooks {
        is_cancelled: &|| false,
        free_bytes: &|_| None,
        on_progress: &mut |_| {},
        explain: &explain,
    };
    let s = run_plan(&steps, &ours, MetaMode::Remove, &RunOptions::new(false), &mut hooks).unwrap();
    assert_eq!((s.done, s.failed, s.skipped), (2, 0, 1));
    assert_eq!(xmp_presence(&a).unwrap(), XmpPresence::None);
}

#[test]
fn file_facts_come_from_the_photo_row() {
    let fx = Fixture::new();
    let bytes = png_bytes(10, 8);
    let path = put(&fx, "shot.png", &bytes);
    scan_folders(&fx.conn, &[photos_dir(&fx)], None).unwrap();
    let id: i64 = fx.conn.query_row("SELECT id FROM photos", [], |r| r.get(0)).unwrap();

    let row = MetadataService::new(&fx.conn).file_row(id).unwrap().unwrap();
    assert_eq!((row.file_name.as_str(), row.file_size), ("shot.png", bytes.len() as i64));
    assert_eq!(Path::new(&row.path), path);
    assert!(MetadataService::new(&fx.conn).file_row(id + 1000).unwrap().is_none());

    let svc = PhotoService::new(&fx.conn);
    let f = svc.file_ref(id).unwrap().unwrap();
    assert_eq!((f.path.as_str(), f.missing, f.online_only, f.file_size), (row.path.as_str(), false, false, row.file_size));
    assert!(!f.mtime.is_empty());
    assert!(svc.file_ref(id + 1000).unwrap().is_none());
}
