using Dapper;
using Microsoft.Data.Sqlite;
using VrcLogger.Core.Parsing;

namespace VrcLogger.Core.Data;

public sealed class ImportService(SqliteConnection conn, ParseOptions? parseOptions = null)
{
    public sealed record ImportSummary(int Scanned, int Imported, int Skipped);

    public sealed record PruneSummary(int ExpiredDeleted, int NoiseDeleted)
    {
        public int Total => ExpiredDeleted + NoiseDeleted;
    }

    /// <summary>
    /// Deletes raw_events older than <paramref name="retentionDays"/> (0 = keep forever) and
    /// rows matching the noise patterns (so an existing database catches up with the parser's
    /// exclusions). Structured tables are never touched. Caller decides whether to VACUUM.
    /// </summary>
    public PruneSummary Prune(int retentionDays, IEnumerable<string> noisePatterns, DateTime? now = null)
    {
        using var tx = conn.BeginTransaction();
        var expired = 0;
        if (retentionDays > 0)
        {
            var cutoff = Db.Ts((now ?? DateTime.Now).AddDays(-retentionDays));
            expired = conn.Execute("DELETE FROM raw_events WHERE event_time < @cutoff", new { cutoff }, tx);
        }
        var noise = 0;
        foreach (var p in noisePatterns)
        {
            if (string.IsNullOrEmpty(p))
                continue;
            var contains = p[0] == '*';
            var body = contains ? p[1..] : p;
            if (body.Length == 0)
                continue;
            var pattern = (contains ? "%" : "") + Db.EscapeLike(body) + "%";
            noise += conn.Execute("DELETE FROM raw_events WHERE message LIKE @pattern ESCAPE '\\'", new { pattern }, tx);
        }
        tx.Commit();
        return new PruneSummary(expired, noise);
    }

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

    /// <summary>
    /// Marks every known file as not yet imported so the next <see cref="ImportFolders"/> reparses
    /// all of them (e.g. after the parser learned to extract new event types).
    /// </summary>
    public int MarkAllForReimport() => conn.Execute("UPDATE log_files SET last_size = 0");

    /// <summary>
    /// Recovers avatar switches from raw_events lines still in the database (the log files
    /// themselves may already be gone). Sizes cannot be recovered: the download lines are
    /// on the noise list. Idempotent; one row per (visit, player, avatar).
    /// </summary>
    public int BackfillAvatarEventsFromRawEvents()
    {
        const string prefix = "[Behaviour] Switching ";
        return conn.Execute("""
            INSERT INTO avatar_events (file_id, visit_id, player_id, display_name, avatar_name, event_time)
            SELECT r.file_id, r.visit_id,
                   (SELECT e.player_id FROM encounters e WHERE e.visit_id = r.visit_id AND e.display_name = s.name LIMIT 1),
                   s.name, s.avatar, r.event_time
            FROM raw_events r
            JOIN (SELECT id,
                         substr(message, @plen + 1, instr(message, ' to avatar ') - @plen - 1) AS name,
                         substr(message, instr(message, ' to avatar ') + 11) AS avatar
                  FROM raw_events
                  WHERE message LIKE @like ESCAPE '\' AND instr(message, ' to avatar ') > @plen) s ON s.id = r.id
            WHERE NOT EXISTS (
                SELECT 1 FROM avatar_events a
                WHERE a.visit_id IS r.visit_id AND a.display_name = s.name AND a.avatar_name = s.avatar
                  AND (r.visit_id IS NOT NULL OR a.file_id = r.file_id))
            GROUP BY r.visit_id, s.name, s.avatar
            """, new { plen = prefix.Length, like = Db.EscapeLike(prefix) + "% to avatar %" });
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
            result = LogParser.Parse(reader, parseOptions);

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
        // display name -> player id, per visit, for linking avatar events (which only carry names).
        var playersByName = new Dictionary<string, long>[result.Visits.Count];
        for (var i = 0; i < result.Visits.Count; i++)
        {
            playersByName[i] = new Dictionary<string, long>(StringComparer.Ordinal);
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
                playersByName[i][enc.DisplayName] = playerId;
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

        // Notifications are not tied to a file: the same notification id is re-logged on every
        // login, so it may appear in several files and must survive any one of them being reimported.
        foreach (var n in result.Notifications)
            conn.Execute("""
                INSERT INTO notifications (notification_id, kind, sender_user_id, sender_name, message, details, created_at_utc, event_time)
                VALUES (@NotificationId, @Kind, @SenderUserId, @SenderName, @Message, @Details, @CreatedAtUtc, @time)
                ON CONFLICT(notification_id) DO UPDATE SET event_time = min(event_time, excluded.event_time)
                """, new { n.NotificationId, n.Kind, n.SenderUserId, n.SenderName, n.Message, n.Details, n.CreatedAtUtc, time = Db.Ts(n.Time) }, tx);

        foreach (var (notificationId, t) in result.AnsweredNotifications)
            conn.Execute("""
                UPDATE notifications SET accepted_at = coalesce(accepted_at, @time) WHERE notification_id = @notificationId
                """, new { notificationId, time = Db.Ts(t) }, tx);

        foreach (var me in result.ModerationEvents)
            conn.Execute("""
                INSERT INTO moderation_events (file_id, visit_id, kind, target_name, event_time)
                VALUES (@fileId, @visitId, @Kind, @TargetName, @time)
                """,
                new
                {
                    fileId,
                    visitId = me.VisitIndex is { } mvi ? visitIds[mvi] : (long?)null,
                    me.Kind, me.TargetName, time = Db.Ts(me.Time),
                }, tx);

        foreach (var av in result.AvatarEvents)
        {
            long? playerId = null;
            if (av.VisitIndex is { } avi && playersByName[avi].TryGetValue(av.PlayerName, out var pid))
                playerId = pid;
            else
            {
                // Outside a visit (or name not among encounters): fall back to a unique current display name.
                var ids = conn.Query<long>("SELECT id FROM players WHERE display_name = @name LIMIT 2",
                    new { name = av.PlayerName }, tx).ToList();
                if (ids.Count == 1)
                    playerId = ids[0];
            }
            conn.Execute("""
                INSERT INTO avatar_events (file_id, visit_id, player_id, display_name, avatar_name, size_mb, load_seconds, event_time)
                VALUES (@fileId, @visitId, @playerId, @PlayerName, @AvatarName, @SizeMb, @LoadSeconds, @time)
                """,
                new
                {
                    fileId,
                    visitId = av.VisitIndex is { } vi2 ? visitIds[vi2] : (long?)null,
                    playerId, av.PlayerName, av.AvatarName, av.SizeMb, av.LoadSeconds, time = Db.Ts(av.Time),
                }, tx);
        }

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
