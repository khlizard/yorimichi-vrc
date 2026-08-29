using Dapper;
using Microsoft.Data.Sqlite;
using VrcLogger.Core.Data;

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
}
