using System.Text.Json;
using Dapper;
using Microsoft.Data.Sqlite;
using VrcLogger.Core.Data;
using VrcLogger.Core.Export;

namespace VrcLogger.Core.Tests;

public class ViewerExporterTests : IDisposable
{
    private readonly string _dir = Directory.CreateTempSubdirectory("vrclogger-test-").FullName;
    private readonly SqliteConnection _conn;
    private const string P = "Debug      -  ";
    private const string EvilName = "</script><b>x";

    public ViewerExporterTests()
    {
        _conn = Db.Open(Path.Combine(_dir, "test.db"));
        Db.EnsureCreated(_conn);

        var log = $$$"""
            2026.06.23 21:38:17 {{{P}}}User Authenticated: Tester (usr_aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa)
            2026.06.23 21:38:23 {{{P}}}[Behaviour] Entering Room: World One
            2026.06.23 21:38:23 {{{P}}}[Behaviour] Joining wrld_bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb:1~region(jp)
            2026.06.23 21:38:33 {{{P}}}[Behaviour] OnPlayerJoined Tester (usr_aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa)
            2026.06.23 21:39:00 {{{P}}}[Behaviour] OnPlayerJoined {{{EvilName}}} (usr_11111111-1111-1111-1111-111111111111)
            2026.06.23 21:45:00 {{{P}}}Received Notification: <Notification from username:Friend A, sender user id:usr_11111111-1111-1111-1111-111111111111 to usr_aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa of type: boop, id: not_11111111-1111-1111-1111-111111111111, created at: 06/23/2026 12:39:00 UTC, details: {{}}, type:boop, m seen:False, message: "">
            2026.06.23 22:00:00 {{{P}}}[Behaviour] OnLeftRoom
            2026.06.23 22:10:00 {{{P}}}[Behaviour] Entering Room: World Two
            2026.06.23 22:10:00 {{{P}}}[Behaviour] Joining wrld_cccccccc-cccc-cccc-cccc-cccccccccccc:2
            2026.06.23 22:10:10 {{{P}}}[Behaviour] OnPlayerJoined テスト太郎 (usr_22222222-2222-2222-2222-222222222222)
            2026.06.23 22:30:00 {{{P}}}[Behaviour] OnLeftRoom

            """;
        var path = Path.Combine(_dir, "output_log_2026-06-23_21-38-10.txt");
        File.WriteAllText(path, log);
        new ImportService(_conn).ImportFile(path);
    }

    public void Dispose()
    {
        _conn.Dispose();
        SqliteConnection.ClearAllPools();
        Directory.Delete(_dir, recursive: true);
    }

    [Fact]
    public void ExportsHtmlWithEmbeddedJson()
    {
        var now = new DateTime(2026, 6, 24, 1, 2, 3);
        var output = Path.Combine(_dir, "out.html");

        ViewerExporter.Export(_conn, output, now);

        Assert.True(File.Exists(output));
        Assert.False(File.Exists(output + ".tmp"));
        var html = File.ReadAllText(output);
        Assert.DoesNotContain(ViewerExporter.Placeholder, html);

        var json = ViewerExporter.BuildJson(_conn, now);
        Assert.Contains(json, html);
        Assert.DoesNotContain(EvilName, html);

        using var doc = JsonDocument.Parse(json);
        var root = doc.RootElement;
        Assert.Equal("2026-06-24 01:02:03", root.GetProperty("generatedAt").GetString());
        Assert.Equal(_conn.ExecuteScalar<long>("SELECT count(*) FROM players"), root.GetProperty("players").GetArrayLength());
        Assert.Equal(_conn.ExecuteScalar<long>("SELECT count(*) FROM visits"), root.GetProperty("visits").GetArrayLength());
        Assert.Equal(_conn.ExecuteScalar<long>("SELECT count(*) FROM encounters"), root.GetProperty("enc").GetArrayLength());
        Assert.Equal(_conn.ExecuteScalar<long>("SELECT count(*) FROM notifications"), root.GetProperty("notif").GetArrayLength());
        Assert.True(root.GetProperty("notif").GetArrayLength() >= 1);
        Assert.Equal(2, root.GetProperty("visits").GetArrayLength());

        var players = root.GetProperty("players").EnumerateArray().ToList();
        var local = players.Single(p => p[1].GetString() == "usr_aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa");
        Assert.Equal(1, local[3].GetInt32());
        Assert.Contains(players, p => p[2].GetString() == EvilName);
        Assert.Contains(players, p => p[2].GetString() == "テスト太郎");
        Assert.Contains("テスト太郎", json); // Japanese stays unescaped
    }

    [Fact]
    public void SecondExportOverwrites()
    {
        var output = Path.Combine(_dir, "out.html");
        var now1 = new DateTime(2026, 6, 24, 1, 2, 3);
        var now2 = new DateTime(2026, 6, 25, 4, 5, 6);

        ViewerExporter.Export(_conn, output, now1);
        ViewerExporter.Export(_conn, output, now2);

        var html = File.ReadAllText(output);
        Assert.Contains("2026-06-25 04:05:06", html);
        Assert.DoesNotContain("2026-06-24 01:02:03", html);
        Assert.False(File.Exists(output + ".tmp"));
    }

    [Fact]
    public void CreatesMissingDirectory()
    {
        var output = Path.Combine(_dir, "sub", "deeper", "out.html");

        ViewerExporter.Export(_conn, output, new DateTime(2026, 6, 24, 1, 2, 3));

        Assert.True(File.Exists(output));
        Assert.False(File.Exists(output + ".tmp"));
    }
}
