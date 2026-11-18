using Dapper;
using Microsoft.Data.Sqlite;
using VrcLogger.Core.Data;

namespace VrcLogger.Core.Tests;

public class QueryPagingTests : IDisposable
{
    private readonly string _dir = Directory.CreateTempSubdirectory("vrclogger-test-").FullName;
    private readonly SqliteConnection _conn;
    private readonly QueryService _q;

    private const string P = "Debug      -  ";
    private const string W = "Warning    -  ";

    public QueryPagingTests()
    {
        _conn = Db.Open(Path.Combine(_dir, "test.db"));
        Db.EnsureCreated(_conn);
        var path = Path.Combine(_dir, "output_log_2026-06-01_21-00-00.txt");
        File.WriteAllText(path, Log());
        new ImportService(_conn).ImportFile(path);
        _q = new QueryService(_conn);
    }

    public void Dispose()
    {
        _conn.Dispose();
        SqliteConnection.ClearAllPools();
        Directory.Delete(_dir, recursive: true);
    }

    // A: 3 visits, B: visits 1-2, C: visit 3 (after A). Local user is in every visit.
    private static string Log() =>
        $"""
        2026.06.01 20:59:00 {P}User Authenticated: Me (usr_aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa)
        2026.06.01 21:00:00 {P}[Behaviour] Joining wrld_bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb:1
        2026.06.01 21:00:05 {P}[Behaviour] OnPlayerJoined Me (usr_aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa)
        2026.06.01 21:00:10 {P}[Behaviour] OnPlayerJoined Alice (usr_11111111-1111-1111-1111-111111111111)
        2026.06.01 21:00:20 {P}[Behaviour] OnPlayerJoined Bob (usr_22222222-2222-2222-2222-222222222222)
        2026.06.01 21:00:20 {W}same second warning one
        2026.06.01 21:00:20 {W}same second warning two
        2026.06.10 21:00:00 {P}[Behaviour] Joining wrld_bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb:2
        2026.06.10 21:00:05 {P}[Behaviour] OnPlayerJoined Me (usr_aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa)
        2026.06.10 21:00:10 {P}[Behaviour] OnPlayerJoined Alice (usr_11111111-1111-1111-1111-111111111111)
        2026.06.10 21:00:20 {P}[Behaviour] OnPlayerJoined Bob (usr_22222222-2222-2222-2222-222222222222)
        2026.06.20 21:00:00 {P}[Behaviour] Joining wrld_bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb:3
        2026.06.20 21:00:05 {P}[Behaviour] OnPlayerJoined Me (usr_aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa)
        2026.06.20 21:00:10 {P}[Behaviour] OnPlayerJoined Alice (usr_11111111-1111-1111-1111-111111111111)
        2026.06.20 21:00:30 {P}[Behaviour] OnPlayerJoined Carol (usr_33333333-3333-3333-3333-333333333333)

        """;

    [Fact]
    public void RecentPlayersExcludeLocalAndOrderByLastSeen()
    {
        var all = _q.GetRecentPlayers(null, null, 0, 100);
        Assert.Equal(["Carol", "Alice", "Bob"], all.Select(p => p.DisplayName).ToArray());
        Assert.All(all, p => Assert.False(p.IsLocal));
        Assert.Equal(3, all[1].EncounterCount);
        Assert.StartsWith("2026-06-20", all[1].LastSeen);
    }

    [Fact]
    public void RecentPlayersPagesConcatenate()
    {
        var all = _q.GetRecentPlayers(null, null, 0, 100);
        var paged = new List<PlayerHit>();
        for (var off = 0; ; off += 2)
        {
            var page = _q.GetRecentPlayers(null, null, off, 2);
            paged.AddRange(page);
            if (page.Count < 2) break;
        }
        Assert.Equal(all.Select(p => p.Id), paged.Select(p => p.Id));
        Assert.Equal(paged.Count, paged.Select(p => p.Id).Distinct().Count());
    }

    [Fact]
    public void RecentPlayersRespectDateFilter()
    {
        var hits = _q.GetRecentPlayers(new DateTime(2026, 6, 5), new DateTime(2026, 6, 12), 0, 100);
        Assert.Equal(["Alice", "Bob"], hits.Select(p => p.DisplayName).OrderBy(n => n).ToArray());
        Assert.All(hits, p => Assert.Equal(1, p.EncounterCount));
    }

    [Fact]
    public void SearchPlayersPagesAndFillsLastSeen()
    {
        var all = _q.SearchPlayers("");
        Assert.Equal(4, all.Count); // includes local user
        var paged = _q.SearchPlayers("", 0, 3).Concat(_q.SearchPlayers("", 3, 3)).ToList();
        Assert.Equal(all.Select(p => p.Id), paged.Select(p => p.Id));
        Assert.All(all, p => Assert.False(string.IsNullOrEmpty(p.LastSeen)));
        Assert.Single(_q.SearchPlayers("Car", 0, 10));
    }

    private List<RawEventHit> PageAll(string term, string? level, int size)
    {
        var result = new List<RawEventHit>();
        (string, long)? before = null;
        while (true)
        {
            var page = _q.SearchRawEvents(term, level, null, null, before, size);
            result.AddRange(page);
            if (page.Count < size) break;
            before = (page[^1].EventTime, page[^1].Id);
        }
        return result;
    }

    private static void AssertDescending(List<RawEventHit> rows)
    {
        for (var i = 1; i < rows.Count; i++)
        {
            var c = string.CompareOrdinal(rows[i - 1].EventTime, rows[i].EventTime);
            Assert.True(c > 0 || (c == 0 && rows[i - 1].Id > rows[i].Id));
        }
    }

    [Fact]
    public void RawEventsEmptyTermPagesEveryRowOnce()
    {
        var total = _conn.ExecuteScalar<long>("SELECT count(*) FROM raw_events");
        var first = _q.SearchRawEvents("", null, null, null, null, 3);
        Assert.Equal(3, first.Count);
        var newest = _conn.Query<long>("SELECT id FROM raw_events ORDER BY event_time DESC, id DESC LIMIT 3").ToList();
        Assert.Equal(newest, first.Select(r => r.Id).ToList());

        var all = PageAll("", null, 3);
        Assert.Equal(total, all.Count);
        Assert.Equal(all.Count, all.Select(r => r.Id).Distinct().Count());
        AssertDescending(all);
    }

    [Fact]
    public void RawEventsPageWithTermAndLevel()
    {
        var expectedTerm = _conn.ExecuteScalar<long>("SELECT count(*) FROM raw_events WHERE message LIKE '%OnPlayerJoined%'");
        var byTerm = PageAll("OnPlayerJoined", null, 2);
        Assert.Equal(expectedTerm, byTerm.Count);
        Assert.Equal(byTerm.Count, byTerm.Select(r => r.Id).Distinct().Count());
        AssertDescending(byTerm);

        var byLevel = PageAll("", "Warning", 1);
        Assert.Equal(2, byLevel.Count);
        Assert.All(byLevel, r => Assert.Equal("Warning", r.Level));
        Assert.Equal(2, byLevel.Select(r => r.EventTime).Distinct().Count() + 1); // same second, two rows
    }
}
