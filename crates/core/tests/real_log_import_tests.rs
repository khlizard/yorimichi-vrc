//! Imports the real logs in `logs/` (git-ignored); returns early when there are none.

mod common;

use common::Fixture;
use yorimichi_core::data::{ImportService, QueryService};

#[test]
fn imports_all_sample_logs_end_to_end() {
    if common::sample_logs().is_empty() {
        eprintln!("SKIPPED: no local logs in logs/ - nothing to verify");
        return;
    }

    let fx = Fixture::new();
    let logs_dir = common::logs_dir();
    let summary = ImportService::new(&fx.conn).import_folders(&[&logs_dir], None).unwrap();

    assert!(summary.scanned > 0);
    assert_eq!(summary.scanned, summary.imported);

    let files = fx.count("SELECT count(*) FROM log_files");
    let visits = fx.count("SELECT count(*) FROM visits");
    assert_eq!(summary.scanned as i64, files);
    assert!(visits > 0);
    assert!(fx.count("SELECT count(*) FROM encounters") >= visits);
    // Each file's last visit is cut off at EOF.
    assert_eq!(files, fx.count("SELECT count(*) FROM visits WHERE ended_cleanly = 0"));
    assert_eq!(1, fx.count("SELECT count(*) FROM players WHERE is_local = 1"));
    assert!(fx.count("SELECT count(*) FROM raw_events") > 0);
    assert_eq!(0, fx.count("SELECT count(*) FROM raw_events WHERE level = 'Error'"));
    assert_eq!(
        fx.count("SELECT count(*) FROM raw_events"),
        fx.count("SELECT count(*) FROM raw_events_fts")
    );

    // Second run: everything unchanged -> nothing reimported.
    let again = ImportService::new(&fx.conn).import_folders(&[&logs_dir], None).unwrap();
    assert_eq!(again.imported, 0);
    assert_eq!(again.skipped, summary.scanned);
    assert_eq!(visits, fx.count("SELECT count(*) FROM visits"));

    // Spot checks used by the GUI: the local user appears in every visit.
    let q = QueryService::new(&fx.conn);
    let local_id = fx.count("SELECT id FROM players WHERE is_local = 1");
    assert_eq!(visits, q.get_encounters(local_id, None, None).unwrap().len() as i64);
    assert!(!q.search_raw_events("Behaviour", None, None, None, None, 2000).unwrap().is_empty());
}
