//! Rust-only extras (not part of the 31 ported xUnit tests): schema/FTS5 and helper checks.

mod common;

use common::{dt, Fixture};
use vrclogger_core::data::db;
use vrclogger_core::data::dtos::format_duration;

#[test]
fn schema_has_fts5_table_and_wal() {
    let fx = Fixture::new();
    let sql: String = fx
        .conn
        .query_row("SELECT sql FROM sqlite_master WHERE name = 'raw_events_fts'", [], |r| r.get(0))
        .unwrap();
    assert!(sql.contains("fts5"));
    let mode: String = fx.conn.query_row("PRAGMA journal_mode", [], |r| r.get(0)).unwrap();
    assert_eq!(mode, "wal");
    let fk: i64 = fx.conn.query_row("PRAGMA foreign_keys", [], |r| r.get(0)).unwrap();
    assert_eq!(fk, 1);
    db::ensure_created(&fx.conn).unwrap(); // idempotent
    assert!(db::file_size_bytes(&fx.conn).unwrap() > 0);
    db::vacuum(&fx.conn).unwrap();
}

#[test]
fn wal_size_is_limited_and_checkpoint_truncates_it() {
    let fx = Fixture::new();
    let limit: i64 = fx.conn.query_row("PRAGMA journal_size_limit", [], |r| r.get(0)).unwrap();
    assert_eq!(limit, db::WAL_SIZE_LIMIT_BYTES);

    fx.conn
        .execute_batch("CREATE TABLE t(x TEXT); WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 2000) INSERT INTO t SELECT hex(randomblob(100)) FROM n;")
        .unwrap();
    let wal = fx.dir.path().join("test.db-wal");
    assert!(std::fs::metadata(&wal).unwrap().len() > 0);
    db::checkpoint_truncate(&fx.conn).unwrap();
    assert_eq!(std::fs::metadata(&wal).unwrap().len(), 0);
}

#[test]
fn ts_escape_like_and_duration_formats() {
    assert_eq!(db::ts(dt(2026, 6, 24, 1, 2, 3)), "2026-06-24 01:02:03");
    assert_eq!(db::escape_like(r"a%b_c\d"), r"a\%b\_c\\d");
    assert_eq!(format_duration("2026-06-23 21:00:00", "2026-06-23 22:05:09"), "1h 05m");
    assert_eq!(format_duration("2026-06-23 21:00:00", "2026-06-23 21:03:09"), "3m 09s");
    assert_eq!(format_duration("2026-06-23 22:00:00", "2026-06-23 21:00:00"), "");
    assert_eq!(format_duration("x", "2026-06-23 21:00:00"), "");
}
