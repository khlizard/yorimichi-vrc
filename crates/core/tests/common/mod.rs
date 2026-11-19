//! Shared helpers for the integration tests (not a test target itself).
#![allow(dead_code)]

use std::path::PathBuf;

use chrono::{NaiveDate, NaiveDateTime};
use rusqlite::Connection;
use tempfile::TempDir;
use vrclogger_core::data::db;

/// Log line prefix after the timestamp (`Debug      -  `).
pub const P: &str = "Debug      -  ";
/// Same for warnings.
pub const W: &str = "Warning    -  ";

/// Builds log text from an indented block: every line is trimmed, blank lines are dropped,
/// the literal tokens `{P}` / `{W}` are replaced by the level prefixes, and a trailing
/// newline is added.
pub fn log(text: &str) -> String {
    let mut s = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(|l| l.replace("{P}", P).replace("{W}", W))
        .collect::<Vec<_>>()
        .join("\n");
    s.push('\n');
    s
}

pub fn d(y: i32, m: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, day).unwrap()
}

pub fn dt(y: i32, m: u32, day: u32, h: u32, mi: u32, s: u32) -> NaiveDateTime {
    d(y, m, day).and_hms_opt(h, mi, s).unwrap()
}

/// A temp directory plus an open, schema-initialised database in it.
/// (`conn` is declared first so it is closed before the directory is removed.)
pub struct Fixture {
    pub conn: Connection,
    pub dir: TempDir,
}

impl Fixture {
    pub fn new() -> Self {
        let dir = tempfile::Builder::new().prefix("vrclogger-test-").tempdir().unwrap();
        let conn = db::open(dir.path().join("test.db")).unwrap();
        db::ensure_created(&conn).unwrap();
        Fixture { conn, dir }
    }

    pub fn write_log(&self, name: &str, content: &str) -> PathBuf {
        let path = self.dir.path().join(name);
        std::fs::write(&path, content).unwrap();
        path
    }

    pub fn count(&self, sql: &str) -> i64 {
        self.conn.query_row(sql, [], |r| r.get(0)).unwrap()
    }
}

/// `<repo root>/logs` (git-ignored folder with real VRChat logs).
pub fn logs_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../logs")
}

/// Real `output_log_*.txt` files in [`logs_dir`] (empty if the folder is missing).
pub fn sample_logs() -> Vec<PathBuf> {
    let Ok(rd) = std::fs::read_dir(logs_dir()) else { return Vec::new() };
    let mut v: Vec<PathBuf> = rd
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("output_log_") && n.ends_with(".txt"))
        })
        .collect();
    v.sort();
    v
}
