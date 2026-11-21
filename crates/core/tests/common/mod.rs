//! Shared helpers for the integration tests (not a test target itself).
#![allow(dead_code)]

use std::path::PathBuf;

use chrono::{NaiveDate, NaiveDateTime};
use rusqlite::Connection;
use tempfile::TempDir;
use yorimichi_core::data::db;

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
        let dir = tempfile::Builder::new().prefix("yorimichi-test-").tempdir().unwrap();
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

// ---- library / timeline / story helpers ----

pub const LOCAL_USR: &str = "usr_22222222-2222-2222-2222-222222222222";
pub const USR_A: &str = "usr_11111111-1111-1111-1111-111111111111";
pub const USR_B: &str = "usr_33333333-3333-3333-3333-333333333333";

/// One world visit with the (non-local) players in it.
pub struct V<'a> {
    /// `YYYY.MM.DD` of the join; a leave on the next day is written as `end_date`.
    pub date: &'a str,
    pub start: &'a str,
    pub end_date: &'a str,
    pub end: &'a str,
    pub world: &'a str,
    pub wrld: &'a str,
    /// `(display name, user id, join HH:MM:SS, leave HH:MM:SS)`, same day as `date`.
    pub players: Vec<(&'a str, &'a str, &'a str, &'a str)>,
}

impl<'a> V<'a> {
    pub fn new(date: &'a str, start: &'a str, end: &'a str, world: &'a str, wrld: &'a str) -> Self {
        V { date, start, end_date: date, end, world, wrld, players: Vec::new() }
    }

    pub fn with(mut self, name: &'a str, usr: &'a str, join: &'a str, leave: &'a str) -> Self {
        self.players.push((name, usr, join, leave));
        self
    }

    pub fn text(&self) -> String {
        let d = self.date;
        let mut lines: Vec<(String, String)> = Vec::new();
        for (name, usr, join, leave) in &self.players {
            lines.push((format!("{d} {join}"), format!("{{P}}[Behaviour] OnPlayerJoined {name} ({usr})")));
            lines.push((format!("{d} {leave}"), format!("{{P}}[Behaviour] OnPlayerLeft {name} ({usr})")));
        }
        lines.sort_by(|a, b| a.0.cmp(&b.0));
        let mut s = format!(
            "{d} {st} {{P}}User Authenticated: Tester ({LOCAL_USR})\n\
             {d} {st} {{P}}[Behaviour] Entering Room: {w}\n\
             {d} {st} {{P}}[Behaviour] Joining {wr}:1\n\
             {d} {st} {{P}}[Behaviour] OnPlayerJoined Tester ({LOCAL_USR})\n",
            st = self.start,
            w = self.world,
            wr = self.wrld
        );
        for (t, l) in lines {
            s.push_str(&format!("{t} {l}\n"));
        }
        s.push_str(&format!("{} {} {{P}}[Behaviour] OnLeftRoom\n", self.end_date, self.end));
        log(&s)
    }
}

/// Imports every visit as its own log file.
pub fn import_visits(fx: &Fixture, visits: &[V]) {
    for (i, v) in visits.iter().enumerate() {
        let path = fx.write_log(&format!("output_log_2026-01-01_00-00-{i:02}.txt"), &v.text());
        yorimichi_core::data::ImportService::new(&fx.conn).import_file(&path, None).unwrap();
    }
}

pub fn player_id(fx: &Fixture, usr: &str) -> i64 {
    fx.conn.query_row("SELECT id FROM players WHERE user_id = ?1", [usr], |r| r.get(0)).unwrap()
}

pub fn world_ref(fx: &Fixture, wrld: &str) -> i64 {
    fx.conn.query_row("SELECT id FROM worlds WHERE world_id = ?1", [wrld], |r| r.get(0)).unwrap()
}

/// Inserts a photo row (file need not exist) and re-links visits. Returns the photo id.
pub fn add_photo(fx: &Fixture, name: &str, taken_at: &str) -> i64 {
    let path = fx.dir.path().join("photos").join(name);
    fx.conn
        .execute(
            "INSERT INTO photos(path, file_name, file_size, mtime, taken_at, time_source, width, height)
             VALUES (?1, ?2, 0, '2026-01-01 00:00:00', ?3, 'name', 4, 3)",
            rusqlite::params![path.to_string_lossy(), name, taken_at],
        )
        .unwrap();
    let id = fx.conn.last_insert_rowid();
    yorimichi_core::photos::relink_visits(&fx.conn).unwrap();
    id
}

/// Like [`add_photo`] but also writes a small PNG at the path.
pub fn add_real_photo(fx: &Fixture, name: &str, taken_at: &str) -> i64 {
    let id = add_photo(fx, name, taken_at);
    let path = fx.dir.path().join("photos").join(name);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    image::RgbImage::new(40, 30).save(&path).unwrap();
    id
}
