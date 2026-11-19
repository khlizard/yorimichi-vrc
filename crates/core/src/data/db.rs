//! SQLite connection setup and schema. Port of `Db.cs`; the SQL is identical so the
//! `vrclog.db` created by the C# app opens without migration.

use std::path::Path;
use std::time::Duration;

use chrono::NaiveDateTime;
use rusqlite::Connection;

use crate::Result;

/// Opens (creating if needed) the database with foreign keys on, a 30 s busy timeout
/// (import/VACUUM on another connection may hold the lock) and WAL journaling.
/// Call [`ensure_created`] afterwards.
pub fn open(db_path: impl AsRef<Path>) -> Result<Connection> {
    let conn = Connection::open(db_path)?;
    conn.busy_timeout(Duration::from_secs(30))?;
    conn.pragma_update(None, "foreign_keys", true)?;
    // journal_mode returns a row, so read it back instead of executing blindly.
    conn.query_row("PRAGMA journal_mode=WAL;", [], |r| r.get::<_, String>(0))?;
    // Without a limit the -wal file keeps its high-water size (hundreds of MB after a re-import).
    conn.query_row(&format!("PRAGMA journal_size_limit={WAL_SIZE_LIMIT_BYTES};"), [], |r| r.get::<_, i64>(0))?;
    Ok(conn)
}

/// Size the `-wal` file is truncated back to after a checkpoint.
pub const WAL_SIZE_LIMIT_BYTES: i64 = 32 * 1024 * 1024;

/// Copies the whole WAL into the database file and truncates the `-wal` file to 0 bytes.
pub fn checkpoint_truncate(conn: &Connection) -> Result<()> {
    conn.query_row("PRAGMA wal_checkpoint(TRUNCATE);", [], |_| Ok(()))?;
    Ok(())
}

/// Creates all tables, FTS5 index, triggers and indices if they do not exist yet.
pub fn ensure_created(conn: &Connection) -> Result<()> {
    conn.execute_batch(SCHEMA)?;
    Ok(())
}

/// Rebuilds the database file so space freed by deletes is returned to the OS.
pub fn vacuum(conn: &Connection) -> Result<()> {
    conn.execute_batch("VACUUM;")?;
    Ok(())
}

/// Logical database size in bytes (`page_count * page_size`).
pub fn file_size_bytes(conn: &Connection) -> Result<i64> {
    Ok(conn.query_row(
        "SELECT page_count * page_size FROM pragma_page_count(), pragma_page_size();",
        [],
        |r| r.get::<_, i64>(0),
    )?)
}

/// Formats a timestamp as stored in the database: `yyyy-MM-dd HH:mm:ss`.
pub fn ts(dt: NaiveDateTime) -> String {
    dt.format("%Y-%m-%d %H:%M:%S").to_string()
}

/// Escapes `\`, `%` and `_` for use in `LIKE ... ESCAPE '\'`.
pub fn escape_like(s: &str) -> String {
    s.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_")
}

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS log_files (
  id INTEGER PRIMARY KEY,
  file_name TEXT NOT NULL UNIQUE,
  last_size INTEGER NOT NULL,
  imported_at TEXT NOT NULL);

CREATE TABLE IF NOT EXISTS worlds (
  id INTEGER PRIMARY KEY,
  world_id TEXT NOT NULL UNIQUE,
  name TEXT NOT NULL);

CREATE TABLE IF NOT EXISTS players (
  id INTEGER PRIMARY KEY,
  user_id TEXT NOT NULL UNIQUE,
  display_name TEXT NOT NULL,
  is_local INTEGER NOT NULL DEFAULT 0);

CREATE TABLE IF NOT EXISTS player_names (
  player_id INTEGER NOT NULL REFERENCES players(id),
  display_name TEXT NOT NULL,
  first_seen TEXT NOT NULL,
  last_seen TEXT NOT NULL,
  PRIMARY KEY (player_id, display_name));

CREATE TABLE IF NOT EXISTS visits (
  id INTEGER PRIMARY KEY,
  file_id INTEGER NOT NULL REFERENCES log_files(id) ON DELETE CASCADE,
  world_id INTEGER REFERENCES worlds(id),
  world_name TEXT NOT NULL,
  instance_id TEXT NOT NULL,
  location TEXT NOT NULL,
  join_time TEXT NOT NULL,
  leave_time TEXT NOT NULL,
  ended_cleanly INTEGER NOT NULL);

CREATE TABLE IF NOT EXISTS encounters (
  id INTEGER PRIMARY KEY,
  visit_id INTEGER NOT NULL REFERENCES visits(id) ON DELETE CASCADE,
  player_id INTEGER NOT NULL REFERENCES players(id),
  display_name TEXT NOT NULL,
  join_time TEXT NOT NULL,
  leave_time TEXT NOT NULL,
  ended_cleanly INTEGER NOT NULL);

CREATE TABLE IF NOT EXISTS url_events (
  id INTEGER PRIMARY KEY,
  visit_id INTEGER REFERENCES visits(id) ON DELETE CASCADE,
  file_id INTEGER NOT NULL REFERENCES log_files(id) ON DELETE CASCADE,
  kind TEXT NOT NULL,
  url TEXT NOT NULL,
  event_time TEXT NOT NULL);

CREATE TABLE IF NOT EXISTS raw_events (
  id INTEGER PRIMARY KEY,
  file_id INTEGER NOT NULL REFERENCES log_files(id) ON DELETE CASCADE,
  visit_id INTEGER REFERENCES visits(id) ON DELETE CASCADE,
  event_time TEXT NOT NULL,
  level TEXT NOT NULL,
  message TEXT NOT NULL);

CREATE VIRTUAL TABLE IF NOT EXISTS raw_events_fts USING fts5(
  message, content='raw_events', content_rowid='id');

CREATE TRIGGER IF NOT EXISTS raw_events_ai AFTER INSERT ON raw_events BEGIN
  INSERT INTO raw_events_fts(rowid, message) VALUES (new.id, new.message);
END;
CREATE TRIGGER IF NOT EXISTS raw_events_ad AFTER DELETE ON raw_events BEGIN
  INSERT INTO raw_events_fts(raw_events_fts, rowid, message) VALUES ('delete', old.id, old.message);
END;

CREATE TABLE IF NOT EXISTS notifications (
  id INTEGER PRIMARY KEY,
  notification_id TEXT NOT NULL UNIQUE,
  kind TEXT NOT NULL,
  sender_user_id TEXT NOT NULL,
  sender_name TEXT NOT NULL,
  message TEXT NOT NULL,
  details TEXT NOT NULL,
  created_at_utc TEXT NOT NULL,
  event_time TEXT NOT NULL,
  accepted_at TEXT);

CREATE TABLE IF NOT EXISTS moderation_events (
  id INTEGER PRIMARY KEY,
  file_id INTEGER NOT NULL REFERENCES log_files(id) ON DELETE CASCADE,
  visit_id INTEGER REFERENCES visits(id) ON DELETE CASCADE,
  kind TEXT NOT NULL,
  target_name TEXT NOT NULL,
  event_time TEXT NOT NULL);

CREATE TABLE IF NOT EXISTS avatar_events (
  id INTEGER PRIMARY KEY,
  file_id INTEGER NOT NULL REFERENCES log_files(id) ON DELETE CASCADE,
  visit_id INTEGER REFERENCES visits(id) ON DELETE CASCADE,
  player_id INTEGER REFERENCES players(id),
  display_name TEXT NOT NULL,
  avatar_name TEXT NOT NULL,
  size_mb REAL,
  load_seconds INTEGER,
  event_time TEXT NOT NULL);

CREATE INDEX IF NOT EXISTS idx_avatar_visit   ON avatar_events(visit_id, display_name, event_time);
CREATE INDEX IF NOT EXISTS idx_avatar_player  ON avatar_events(player_id, event_time);
CREATE INDEX IF NOT EXISTS idx_avatar_name    ON avatar_events(avatar_name);
CREATE INDEX IF NOT EXISTS idx_avatar_file    ON avatar_events(file_id);
CREATE INDEX IF NOT EXISTS idx_notif_time     ON notifications(event_time);
CREATE INDEX IF NOT EXISTS idx_notif_sender   ON notifications(sender_user_id);
CREATE INDEX IF NOT EXISTS idx_mod_time       ON moderation_events(event_time);
CREATE INDEX IF NOT EXISTS idx_mod_file       ON moderation_events(file_id);
CREATE INDEX IF NOT EXISTS idx_mod_visit      ON moderation_events(visit_id);
CREATE INDEX IF NOT EXISTS idx_visits_join     ON visits(join_time);
CREATE INDEX IF NOT EXISTS idx_visits_file     ON visits(file_id);
CREATE INDEX IF NOT EXISTS idx_enc_player_time ON encounters(player_id, join_time);
CREATE INDEX IF NOT EXISTS idx_enc_visit       ON encounters(visit_id);
CREATE INDEX IF NOT EXISTS idx_url_visit       ON url_events(visit_id);
CREATE INDEX IF NOT EXISTS idx_url_url         ON url_events(url);
CREATE INDEX IF NOT EXISTS idx_url_file        ON url_events(file_id);
CREATE INDEX IF NOT EXISTS idx_raw_file        ON raw_events(file_id);
CREATE INDEX IF NOT EXISTS idx_raw_time        ON raw_events(event_time);
CREATE INDEX IF NOT EXISTS idx_raw_visit       ON raw_events(visit_id);
"#;
