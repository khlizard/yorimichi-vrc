using Dapper;
using Microsoft.Data.Sqlite;
using VrcLogger.Core.Data;
using VrcLogger.Core.Parsing;

namespace VrcLogger.Core.Tests;

public class ImportServiceTests : IDisposable
{
    private readonly string _dir = Directory.CreateTempSubdirectory("vrclogger-test-").FullName;
    private readonly SqliteConnection _conn;

    public ImportServiceTests()
    {
        _conn = Db.Open(Path.Combine(_dir, "test.db"));
        Db.EnsureCreated(_conn);
    }

    public void Dispose()
    {
        _conn.Dispose();
        SqliteConnection.ClearAllPools();
        Directory.Delete(_dir, recursive: true);
    }

    private const string P = "Debug      -  ";

    private string WriteLog(string name, string content)
    {
        var path = Path.Combine(_dir, name);
        File.WriteAllText(path, content);
        return path;
    }

    private static string SampleLog(bool withSecondPlayer = false) =>
        $"""
        2026.06.23 21:38:17 {P}User Authenticated: テスト／Tester (usr_aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa)
        2026.06.23 21:38:23 {P}[Behaviour] Entering Room: Sample World
        2026.06.23 21:38:23 {P}[Behaviour] Joining wrld_bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb:91278~region(jp)
        2026.06.23 21:38:33 {P}[Behaviour] OnPlayerJoined テスト／Tester (usr_aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa)
        2026.06.23 21:40:00 {P}[Video Playback] Resolving URL 'https://example.com/video.mp4'

        """ + (withSecondPlayer
            ? $"2026.06.23 21:41:00 {P}[Behaviour] OnPlayerJoined Friend A (usr_11111111-1111-1111-1111-111111111111)\n"
            : "");

    [Fact]
    public void ImportsAndSkipsUnchanged()
    {
        var path = WriteLog("output_log_2026-06-23_21-38-10.txt", SampleLog());
        var svc = new ImportService(_conn);

        Assert.True(svc.ImportFile(path));
        Assert.False(svc.ImportFile(path)); // unchanged → skipped

        Assert.Equal(1, _conn.ExecuteScalar<long>("SELECT count(*) FROM log_files"));
        Assert.Equal(1, _conn.ExecuteScalar<long>("SELECT count(*) FROM visits"));
        Assert.Equal(1, _conn.ExecuteScalar<long>("SELECT count(*) FROM encounters"));
        Assert.Equal(1, _conn.ExecuteScalar<long>("SELECT count(*) FROM url_events"));
        Assert.Equal(1, _conn.ExecuteScalar<long>("SELECT count(*) FROM players WHERE is_local = 1"));
        Assert.Equal(5, _conn.ExecuteScalar<long>("SELECT count(*) FROM raw_events"));
    }

    [Fact]
    public void ReimportsGrownFileWithoutDuplicates()
    {
        var path = WriteLog("output_log_2026-06-23_21-38-10.txt", SampleLog());
        var svc = new ImportService(_conn);
        svc.ImportFile(path);

        File.WriteAllText(path, SampleLog(withSecondPlayer: true));
        Assert.True(svc.ImportFile(path)); // grew → reimported

        Assert.Equal(1, _conn.ExecuteScalar<long>("SELECT count(*) FROM log_files"));
        Assert.Equal(1, _conn.ExecuteScalar<long>("SELECT count(*) FROM visits"));
        Assert.Equal(2, _conn.ExecuteScalar<long>("SELECT count(*) FROM encounters"));
        Assert.Equal(1, _conn.ExecuteScalar<long>("SELECT count(*) FROM url_events"));
        Assert.Equal(6, _conn.ExecuteScalar<long>("SELECT count(*) FROM raw_events"));
        // FTS index followed the delete+reinsert (triggers).
        Assert.Equal(6, _conn.ExecuteScalar<long>("SELECT count(*) FROM raw_events_fts"));
    }

    [Fact]
    public void QueriesReturnImportedData()
    {
        var path = WriteLog("output_log_2026-06-23_21-38-10.txt", SampleLog(withSecondPlayer: true));
        new ImportService(_conn).ImportFile(path);
        var q = new QueryService(_conn);

        var players = q.SearchPlayers("Tester");
        var hit = Assert.Single(players);
        Assert.True(hit.IsLocal);

        var encounters = q.GetEncounters(hit.Id, null, null);
        Assert.Single(encounters);
        Assert.Equal("Sample World", encounters[0].WorldName);

        var visits = q.GetVisits(null, null, null);
        var visit = Assert.Single(visits);
        Assert.Equal(2, visit.PlayerCount);

        Assert.Equal(2, q.GetVisitPlayers(visit.Id).Count);
        Assert.Single(q.GetVisitUrls(visit.Id));
        Assert.Single(q.SearchUrls("example.com", null, null, null));
        Assert.Empty(q.SearchUrls("example.com", "image", null, null));

        // FTS search over raw events.
        Assert.Single(q.SearchRawEvents("Resolving URL", null, null, null));
        Assert.Equal(2, q.SearchRawEvents("OnPlayerJoined", null, null, null).Count);

        // Date filter excludes everything outside the range.
        Assert.Empty(q.GetVisits(null, new DateTime(2027, 1, 1), null));
        Assert.Single(q.GetVisits(null, new DateTime(2026, 6, 23), new DateTime(2026, 6, 23)));
    }

    [Fact]
    public void PlayerRenameKeepsHistory()
    {
        var svc = new ImportService(_conn);
        var log1 = $"""
            2026.06.23 21:00:00 {P}[Behaviour] Joining wrld_11111111-1111-1111-1111-111111111111:1
            2026.06.23 21:00:10 {P}[Behaviour] OnPlayerJoined OldName (usr_11111111-1111-1111-1111-111111111111)
            2026.06.23 22:00:00 {P}[Behaviour] OnLeftRoom
            """;
        var log2 = $"""
            2026.07.01 21:00:00 {P}[Behaviour] Joining wrld_11111111-1111-1111-1111-111111111111:2
            2026.07.01 21:00:10 {P}[Behaviour] OnPlayerJoined NewName (usr_11111111-1111-1111-1111-111111111111)
            2026.07.01 22:00:00 {P}[Behaviour] OnLeftRoom
            """;
        svc.ImportFile(WriteLog("output_log_2026-06-23_21-00-00.txt", log1));
        svc.ImportFile(WriteLog("output_log_2026-07-01_21-00-00.txt", log2));

        var q = new QueryService(_conn);
        var hit = Assert.Single(q.SearchPlayers("OldName")); // old name still searchable
        Assert.Equal("NewName", hit.DisplayName);            // latest name shown
        Assert.Contains("OldName", hit.OtherNames);
        Assert.Equal(2, q.GetEncounters(hit.Id, null, null).Count);
    }

    private static string NotificationLine(string kind, string id, string sender, string senderId, string ts) =>
        $$$"""{{{ts}}} {{{P}}}Received Notification: <Notification from username:{{{sender}}}, sender user id:{{{senderId}}} to usr_aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa of type: {{{kind}}}, id: {{{id}}}, created at: 06/23/2026 12:39:00 UTC, details: {{}}, type:{{{kind}}}, m seen:False, message: "">""";

    [Fact]
    public void NotificationsAreDedupedAcrossFilesAndSurviveReimport()
    {
        var svc = new ImportService(_conn);
        var log1 = SampleLog() + "\n" +
            NotificationLine("boop", "not_11111111-1111-1111-1111-111111111111", "Friend A", "usr_11111111-1111-1111-1111-111111111111", "2026.06.23 21:45:00") + "\n" +
            $"2026.06.23 21:46:00 {P}[ModerationManager] Some One is now Muted\n";
        // Second session re-logs the same notification (VRChat replays recent notifications on login).
        var log2 = $$$"""
            2026.06.24 21:00:00 {{{P}}}[Behaviour] Joining wrld_11111111-1111-1111-1111-111111111111:2
            {{{NotificationLine("boop", "not_11111111-1111-1111-1111-111111111111", "Friend A", "usr_11111111-1111-1111-1111-111111111111", "2026.06.24 21:00:10")}}}
            {{{NotificationLine("friendRequest", "frq_22222222-2222-2222-2222-222222222222", "Friend B", "usr_22222222-2222-2222-2222-222222222222", "2026.06.24 21:00:20")}}}
            2026.06.24 21:01:00 {{{P}}}AcceptNotification for notification:<Notification from username:Friend B, sender user id:usr_22222222-2222-2222-2222-222222222222 to usr_aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa of type: friendRequest, id: frq_22222222-2222-2222-2222-222222222222, created at: 06/23/2026 12:39:00 UTC, details: {{}}, type:friendRequest, m seen:False, message: "">
            2026.06.24 22:00:00 {{{P}}}[Behaviour] OnLeftRoom
            """;
        var path1 = WriteLog("output_log_2026-06-23_21-38-10.txt", log1);
        svc.ImportFile(path1);
        svc.ImportFile(WriteLog("output_log_2026-06-24_21-00-00.txt", log2));

        Assert.Equal(2, _conn.ExecuteScalar<long>("SELECT count(*) FROM notifications"));
        // First sighting wins for event_time.
        Assert.Equal("2026-06-23 21:45:00", _conn.ExecuteScalar<string>(
            "SELECT event_time FROM notifications WHERE notification_id = 'not_11111111-1111-1111-1111-111111111111'"));
        Assert.Equal("2026-06-24 21:01:00", _conn.ExecuteScalar<string>(
            "SELECT accepted_at FROM notifications WHERE notification_id = 'frq_22222222-2222-2222-2222-222222222222'"));
        Assert.Equal(1, _conn.ExecuteScalar<long>("SELECT count(*) FROM moderation_events"));

        // File 1 grows and is reimported: its cascade delete must not take the shared notification with it.
        File.WriteAllText(path1, log1 + $"2026.06.23 21:50:00 {P}[Behaviour] OnLeftRoom\n");
        Assert.True(svc.ImportFile(path1));
        Assert.Equal(2, _conn.ExecuteScalar<long>("SELECT count(*) FROM notifications"));
        Assert.Equal(1, _conn.ExecuteScalar<long>("SELECT count(*) FROM moderation_events")); // replaced, not duplicated

        var q = new QueryService(_conn);
        Assert.Equal(2, q.SearchNotifications("", null, null, null).Count);
        Assert.Single(q.SearchNotifications("Friend B", null, null, null));
        Assert.Single(q.SearchNotifications("", "boop", null, null));
        Assert.Equal("応答済", q.SearchNotifications("", "friendRequest", null, null)[0].Response);
        Assert.Equal(["boop", "friendRequest"], q.GetNotificationKinds());

        var mod = Assert.Single(q.SearchModerationEvents("Some", null, null));
        Assert.Equal("mute", mod.Kind);
        Assert.Equal("Sample World", mod.WorldName);
        Assert.NotNull(mod.VisitId);
    }

    [Fact]
    public void PruneDeletesExpiredAndNoiseRawEventsOnly()
    {
        var svc = new ImportService(_conn, new ParseOptions([])); // import everything so Prune has noise to clean
        var log = $$$"""
            2026.06.01 21:00:00 {{{P}}}[Behaviour] Joining wrld_11111111-1111-1111-1111-111111111111:1
            2026.06.01 21:00:10 {{{P}}}[Behaviour] OnPlayerJoined Old Friend (usr_11111111-1111-1111-1111-111111111111)
            2026.06.01 21:00:20 {{{P}}}[Video Playback] Resolving URL 'https://example.com/old.mp4'
            2026.06.01 22:00:00 {{{P}}}[Behaviour] OnLeftRoom
            2026.07.01 21:00:00 {{{P}}}[Behaviour] Joining wrld_11111111-1111-1111-1111-111111111111:2
            2026.07.01 21:00:10 {{{P}}}[Behaviour] Using default gesture mask (hands)
            2026.07.01 21:00:11 {{{P}}}Some One Avatar will follow Safety Settings
            2026.07.01 21:00:12 {{{P}}}MyWorldScript: 100% done
            2026.07.01 22:00:00 {{{P}}}[Behaviour] OnLeftRoom
            """;
        svc.ImportFile(WriteLog("output_log_2026-06-01_21-00-00.txt", log));
        Assert.Equal(9, _conn.ExecuteScalar<long>("SELECT count(*) FROM raw_events"));

        var pruned = svc.Prune(retentionDays: 30, RawLogNoise.Default, now: new DateTime(2026, 7, 10));

        Assert.Equal(4, pruned.ExpiredDeleted); // everything from June
        Assert.Equal(2, pruned.NoiseDeleted);   // gesture mask + Safety Settings (contains-match)
        var remaining = _conn.Query<string>("SELECT message FROM raw_events ORDER BY event_time").ToList();
        Assert.Equal(
        [
            "[Behaviour] Joining wrld_11111111-1111-1111-1111-111111111111:2",
            "MyWorldScript: 100% done",
            "[Behaviour] OnLeftRoom",
        ], remaining);
        Assert.Equal(3, _conn.ExecuteScalar<long>("SELECT count(*) FROM raw_events_fts")); // triggers kept FTS in sync

        // Structured data is untouched.
        Assert.Equal(2, _conn.ExecuteScalar<long>("SELECT count(*) FROM visits"));
        Assert.Equal(1, _conn.ExecuteScalar<long>("SELECT count(*) FROM encounters"));
        Assert.Equal(1, _conn.ExecuteScalar<long>("SELECT count(*) FROM url_events"));

        // Retention 0 keeps everything; noise list still applies (nothing left to remove).
        Assert.Equal(0, svc.Prune(0, RawLogNoise.Default, now: new DateTime(2030, 1, 1)).Total);
    }

    [Fact]
    public void AvatarEventsAreLinkedToPlayersAndQueryable()
    {
        var svc = new ImportService(_conn);
        var log = $$$"""
            2026.06.23 21:38:23 {{{P}}}[Behaviour] Entering Room: Sample World
            2026.06.23 21:38:23 {{{P}}}[Behaviour] Joining wrld_11111111-1111-1111-1111-111111111111:1
            2026.06.23 21:38:25 {{{P}}}[Behaviour] OnPlayerJoined Alice (usr_11111111-1111-1111-1111-111111111111)
            2026.06.23 21:38:30 {{{P}}}[Behaviour] Switching Alice to avatar First
            2026.06.23 21:38:34 {{{P}}}[AssetBundleDownloadManager] Download for avatar (Worn:0 Dist:9.03) (44.2 MB) started 0 seconds and completed 3 seconds after queueing.
            2026.06.23 21:38:34 {{{P}}}[AssetBundleDownloadManager] [114] Unpacking Avatar (First by Alice)
            2026.06.23 21:50:00 {{{P}}}[Behaviour] Switching Alice to avatar Second
            2026.06.23 22:00:00 {{{P}}}[Behaviour] OnLeftRoom
            """;
        svc.ImportFile(WriteLog("output_log_2026-06-23_21-38-10.txt", log));

        Assert.Equal(2, _conn.ExecuteScalar<long>("SELECT count(*) FROM avatar_events"));
        Assert.Equal(2, _conn.ExecuteScalar<long>("SELECT count(*) FROM avatar_events WHERE player_id IS NOT NULL"));

        var q = new QueryService(_conn);
        var visit = Assert.Single(q.GetVisits(null, null, null));
        var player = Assert.Single(q.GetVisitPlayers(visit.Id));
        Assert.Equal("Second", player.AvatarName);   // latest avatar in the visit
        Assert.Equal(44.2, player.AvatarSizeMb);     // last known size (from the earlier download)

        var hit = Assert.Single(q.SearchPlayers("Alice"));
        var enc = Assert.Single(q.GetEncounters(hit.Id, null, null));
        Assert.Equal("Second", enc.AvatarName);

        var rows = q.SearchAvatars("First", null, null);
        var row = Assert.Single(rows);
        Assert.Equal(("Alice", 44.2, 3, "Sample World"), (row.DisplayName, row.SizeMb, row.LoadSeconds, row.WorldName));
        Assert.Equal(visit.Id, row.VisitId);
        Assert.Equal(2, q.SearchAvatars("alice", null, null).Count); // player name matches too (LIKE is case-insensitive for ASCII)
        Assert.Empty(q.SearchAvatars("First", new DateTime(2027, 1, 1), null));
    }

    [Fact]
    public void BackfillRecoversAvatarSwitchesFromRawEvents()
    {
        var svc = new ImportService(_conn, new ParseOptions([])); // keep every line in raw_events
        var log = $$$"""
            2026.06.23 21:38:23 {{{P}}}[Behaviour] Joining wrld_11111111-1111-1111-1111-111111111111:1
            2026.06.23 21:38:25 {{{P}}}[Behaviour] OnPlayerJoined Alice (usr_11111111-1111-1111-1111-111111111111)
            2026.06.23 21:38:30 {{{P}}}[Behaviour] Switching Alice to avatar Dress by Shop
            2026.06.23 21:38:31 {{{P}}}[Behaviour] Switching Alice to avatar Dress by Shop
            2026.06.23 21:50:00 {{{P}}}[Behaviour] Switching Alice to avatar Second
            2026.06.23 22:00:00 {{{P}}}[Behaviour] OnLeftRoom
            2026.06.23 22:01:00 {{{P}}}[Behaviour] Switching Nobody to avatar Lobby Look
            """;
        svc.ImportFile(WriteLog("output_log_2026-06-23_21-38-10.txt", log));
        Assert.Equal(3, _conn.ExecuteScalar<long>("SELECT count(*) FROM avatar_events"));

        // Simulate a database that predates avatar tracking.
        _conn.Execute("DELETE FROM avatar_events");
        Assert.Equal(3, svc.BackfillAvatarEventsFromRawEvents());
        Assert.Equal(0, svc.BackfillAvatarEventsFromRawEvents()); // idempotent

        var rows = _conn.Query<(string Name, string Avatar, long? VisitId, long? PlayerId)>(
            "SELECT display_name, avatar_name, visit_id, player_id FROM avatar_events ORDER BY event_time").ToList();
        Assert.Equal(["Dress by Shop", "Second", "Lobby Look"], rows.Select(r => r.Avatar).ToList());
        Assert.All(rows.Take(2), r => { Assert.NotNull(r.VisitId); Assert.NotNull(r.PlayerId); });
        Assert.Null(rows[2].VisitId);
    }
}
