using Dapper;
using Microsoft.Data.Sqlite;
using VrcLogger.Core.Data;

namespace VrcLogger.Core.Tests;

public class RealLogImportTests : IDisposable
{
    private readonly string _dir = Directory.CreateTempSubdirectory("vrclogger-real-").FullName;
    private readonly SqliteConnection _conn;

    public RealLogImportTests()
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

    [Fact]
    public void ImportsAllSampleLogsEndToEnd()
    {
        if (!RealLogIntegrationTests.HasSampleLogs)
            return; // no local logs (git-ignored) — nothing to verify

        var logsDir = RealLogIntegrationTests.LogsDir;
        var summary = new ImportService(_conn).ImportFolders([logsDir]);

        Assert.True(summary.Scanned > 0);
        Assert.Equal(summary.Scanned, summary.Imported);

        var files = _conn.ExecuteScalar<long>("SELECT count(*) FROM log_files");
        var visits = _conn.ExecuteScalar<long>("SELECT count(*) FROM visits");
        Assert.Equal(summary.Scanned, files);
        Assert.True(visits > 0);
        Assert.True(_conn.ExecuteScalar<long>("SELECT count(*) FROM encounters") >= visits);
        // Each file's last visit is cut off at EOF.
        Assert.Equal(files, _conn.ExecuteScalar<long>("SELECT count(*) FROM visits WHERE ended_cleanly = 0"));
        Assert.Equal(1, _conn.ExecuteScalar<long>("SELECT count(*) FROM players WHERE is_local = 1"));
        Assert.True(_conn.ExecuteScalar<long>("SELECT count(*) FROM raw_events") > 0);
        Assert.Equal(0, _conn.ExecuteScalar<long>("SELECT count(*) FROM raw_events WHERE level = 'Error'"));
        Assert.Equal(
            _conn.ExecuteScalar<long>("SELECT count(*) FROM raw_events"),
            _conn.ExecuteScalar<long>("SELECT count(*) FROM raw_events_fts"));

        // Second run: everything unchanged → nothing reimported.
        var again = new ImportService(_conn).ImportFolders([logsDir]);
        Assert.Equal(0, again.Imported);
        Assert.Equal(summary.Scanned, again.Skipped);
        Assert.Equal(visits, _conn.ExecuteScalar<long>("SELECT count(*) FROM visits"));

        // Spot checks used by the GUI: the local user appears in every visit.
        var q = new QueryService(_conn);
        var localId = _conn.ExecuteScalar<long>("SELECT id FROM players WHERE is_local = 1");
        Assert.Equal(visits, q.GetEncounters(localId, null, null).Count);
        Assert.NotEmpty(q.SearchRawEvents("Behaviour", null, null, null));
    }
}
