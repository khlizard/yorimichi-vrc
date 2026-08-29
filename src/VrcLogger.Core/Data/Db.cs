using Microsoft.Data.Sqlite;

namespace VrcLogger.Core.Data;

public static class Db
{
    public static SqliteConnection Open(string dbPath)
    {
        var conn = new SqliteConnection(new SqliteConnectionStringBuilder
        {
            DataSource = dbPath,
            ForeignKeys = true,
        }.ToString());
        conn.Open();
        using var cmd = conn.CreateCommand();
        cmd.CommandText = "PRAGMA journal_mode=WAL;";
        cmd.ExecuteNonQuery();
        return conn;
    }

    public static void EnsureCreated(SqliteConnection conn)
    {
        using var cmd = conn.CreateCommand();
        cmd.CommandText = """
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
            """;
        cmd.ExecuteNonQuery();
    }

    public static string Ts(DateTime dt) => dt.ToString("yyyy-MM-dd HH:mm:ss");
}
