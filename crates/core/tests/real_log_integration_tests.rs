//! Tests against real VRChat logs placed in `logs/` at the repo root.
//! That folder is git-ignored (it contains user IDs), so these tests return early
//! (printing a note) when it has no logs.

mod common;

use std::collections::BTreeSet;

use vrclogger_core::parsing::{parse_file, ParseOptions};

#[test]
fn parses_all_sample_logs() {
    let logs = common::sample_logs();
    if logs.is_empty() {
        eprintln!("SKIPPED: no local logs in logs/ - nothing to verify");
        return;
    }

    let options = ParseOptions::default();
    let results: Vec<_> = logs.iter().map(|p| parse_file(p, &options).unwrap().0).collect();
    assert!(!results.is_empty());

    // Every file's last visit is cut off at EOF (VRChat killed / still running).
    for r in &results {
        assert!(!r.visits.last().unwrap().ended_cleanly);
    }

    // Local user is identified in every file and is the same account throughout.
    for r in &results {
        assert!(r.local_user_id.as_deref().unwrap().starts_with("usr_"));
        assert!(!r.local_user_name.as_deref().unwrap_or("").is_empty());
    }
    let ids: BTreeSet<_> = results.iter().map(|r| r.local_user_id.clone()).collect();
    assert_eq!(ids.len(), 1);

    // Every visit has a world id and the local user among its encounters.
    for r in &results {
        for v in &r.visits {
            assert!(v.world_id.as_deref().unwrap().starts_with("wrld_"));
            assert!(v.encounters.iter().any(|e| Some(&e.user_id) == r.local_user_id.as_ref()));
        }
    }

    // No Error-level lines leak into raw events.
    for r in &results {
        assert!(!r.raw_events.iter().any(|e| e.level == "Error"));
    }
}
