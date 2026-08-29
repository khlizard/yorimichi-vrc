using Dapper;
using Microsoft.Data.Sqlite;
using VrcLogger.Core.Parsing;

namespace VrcLogger.Core.Data;

public sealed class ImportService(SqliteConnection conn)
{
    public sealed record ImportSummary(int Scanned, int Imported, int Skipped);

    /// <summary>Scans the given folders for output_log_*.txt and imports new or grown files.</summary>
    public ImportSummary ImportFolders(IEnumerable<string> folders, IProgress<string>? progress = null)
    {
        int scanned = 0, imported = 0, skipped = 0;
        var candidates = new List<string>();
        foreach (var folder in folders)
        {
            if (!Directory.Exists(folder))
            {
                progress?.Report($"フォルダなし(スキップ): {folder}");
                continue;
            }
            candidates.AddRange(Directory.EnumerateFiles(folder, "output_log_*.txt"));
        }
        // Same log may exist in several folders (e.g. a copy in the repo and the live
        // one VRChat is still writing) — import the largest copy of each name.
        var byName = candidates
            .GroupBy(Path.GetFileName)
            .Select(g => g.OrderByDescending(p => new FileInfo(p).Length).First())
            .OrderBy(Path.GetFileName);
        foreach (var path in byName)
        {
            scanned++;
            if (ImportFile(path, progress))
                imported++;
            else
                skipped++;
        }
        progress?.Report($"完了: {imported} 取り込み / {skipped} 変更なし");
        return new ImportSummary(scanned, imported, skipped);
    }

    /// <summary>Returns true if the file was (re)imported, false if unchanged.</summary>
    public bool ImportFile(string path, IProgress<string>? progress = null)
    {
        var name = Path.GetFileName(path);
        // VRChat keeps the newest log open for writing — must allow ReadWrite share.
        using var stream = new FileStream(path, FileMode.Open, FileAccess.Read,
            FileShare.ReadWrite | FileShare.Delete);
        var size = stream.Length;

        var existing = conn.QuerySingleOrDefault<(long Id, long LastSize)?>(
            "SELECT id, last_size FROM log_files WHERE file_name = @name", new { name });
        if (existing is { } e && e.LastSize == size)
            return false;

        progress?.Report($"取り込み中: {name}");
        ParseResult result;
        using (var reader = new StreamReader(stream))
            result = LogParser.Parse(reader);

        using var tx = conn.BeginTransaction();
        if (existing is { } grown)
        {
            // File grew (or was replaced): wipe its rows via CASCADE and reparse from scratch.
            conn.Execute("DELETE FROM log_files WHERE id = @Id", new { grown.Id }, tx);
        }
        var fileId = conn.ExecuteScalar<long>(
            "INSERT INTO log_files (file_name, last_size, imported_at) VALUES (@name, @size, @now) RETURNING id",
            new { name, size, now = Db.Ts(DateTime.Now) }, tx);

        var visitIds = new long[result.Visits.Count];
        for (var i = 0; i < result.Visits.Count; i++)
        {
            var v = result.Visits[i];
            long? worldRowId = v.WorldId is null ? null : conn.ExecuteScalar<long>("""
                INSERT INTO worlds (world_id, name) VALUES (@WorldId, @WorldName)
                ON CONFLICT(world_id) DO UPDATE SET name = CASE WHEN excluded.name <> '' THEN excluded.name ELSE name END
                RETURNING id
                """, new { v.WorldId, v.WorldName }, tx);

            visitIds[i] = conn.ExecuteScalar<long>("""
                INSERT INTO visits (file_id, world_id, world_name, instance_id, location, join_time, leave_time, ended_cleanly)
                VALUES (@fileId, @worldRowId, @WorldName, @InstanceId, @Location, @join, @leave, @clean)
                RETURNING id
                """,
                new
                {
                    fileId, worldRowId, v.WorldName, v.InstanceId, v.Location,
                    join = Db.Ts(v.JoinTime), leave = Db.Ts(v.LeaveTime), clean = v.EndedCleanly,
                }, tx);

            foreach (var enc in v.Encounters)
            {
                var playerId = UpsertPlayer(tx, enc.UserId, enc.DisplayName,
                    isLocal: enc.UserId == result.LocalUserId,
                    firstSeen: Db.Ts(enc.JoinTime), lastSeen: Db.Ts(enc.LeaveTime));
                conn.Execute("""
                    INSERT INTO encounters (visit_id, player_id, display_name, join_time, leave_time, ended_cleanly)
                    VALUES (@visitId, @playerId, @DisplayName, @join, @leave, @clean)
                    """,
                    new
                    {
                        visitId = visitIds[i], playerId, enc.DisplayName,
                        join = Db.Ts(enc.JoinTime), leave = Db.Ts(enc.LeaveTime), clean = enc.EndedCleanly,
                    }, tx);
            }

            foreach (var u in v.UrlEvents)
                InsertUrl(tx, visitIds[i], fileId, u);
        }

        if (result.LocalUserId is not null && result.LocalUserName is not null && result.LastTimestamp is { } last)
            UpsertPlayer(tx, result.LocalUserId, result.LocalUserName, isLocal: true, Db.Ts(last), Db.Ts(last));

        foreach (var u in result.OrphanUrlEvents)
            InsertUrl(tx, null, fileId, u);

        foreach (var raw in result.RawEvents)
            conn.Execute("""
                INSERT INTO raw_events (file_id, visit_id, event_time, level, message)
                VALUES (@fileId, @visitId, @time, @Level, @Message)
                """,
                new
                {
                    fileId,
                    visitId = raw.VisitIndex is { } vi ? visitIds[vi] : (long?)null,
                    time = Db.Ts(raw.Time), raw.Level, raw.Message,
                }, tx);

        tx.Commit();
        return true;
    }

    private long UpsertPlayer(SqliteTransaction tx, string userId, string displayName, bool isLocal, string firstSeen, string lastSeen)
    {
        var playerId = conn.ExecuteScalar<long>("""
            INSERT INTO players (user_id, display_name, is_local) VALUES (@userId, @displayName, @isLocal)
            ON CONFLICT(user_id) DO UPDATE SET
              display_name = CASE WHEN @lastSeen >= (SELECT max(last_seen) FROM player_names WHERE player_id = players.id)
                                  THEN excluded.display_name ELSE display_name END,
              is_local = max(is_local, excluded.is_local)
            RETURNING id
            """, new { userId, displayName, isLocal, lastSeen }, tx);
        conn.Execute("""
            INSERT INTO player_names (player_id, display_name, first_seen, last_seen)
            VALUES (@playerId, @displayName, @firstSeen, @lastSeen)
            ON CONFLICT(player_id, display_name) DO UPDATE SET
              first_seen = min(first_seen, excluded.first_seen),
              last_seen = max(last_seen, excluded.last_seen)
            """, new { playerId, displayName, firstSeen, lastSeen }, tx);
        return playerId;
    }

    private void InsertUrl(SqliteTransaction tx, long? visitId, long fileId, ParsedUrlEvent u) =>
        conn.Execute(
            "INSERT INTO url_events (visit_id, file_id, kind, url, event_time) VALUES (@visitId, @fileId, @Kind, @Url, @time)",
            new { visitId, fileId, u.Kind, u.Url, time = Db.Ts(u.Time) }, tx);
}
