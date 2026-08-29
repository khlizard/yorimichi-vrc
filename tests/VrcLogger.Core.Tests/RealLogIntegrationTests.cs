using VrcLogger.Core.Parsing;

namespace VrcLogger.Core.Tests;

/// <summary>
/// Tests against real VRChat logs placed in <c>logs/</c> at the repo root.
/// That folder is git-ignored (it contains user IDs), so these tests are
/// no-ops when it is absent.
/// </summary>
public class RealLogIntegrationTests
{
    public static string RepoRoot()
    {
        var dir = new DirectoryInfo(AppContext.BaseDirectory);
        while (dir is not null && !File.Exists(Path.Combine(dir.FullName, "VrcLogger.slnx")))
            dir = dir.Parent;
        return dir?.FullName ?? throw new InvalidOperationException("repo root not found");
    }

    public static string LogsDir => Path.Combine(RepoRoot(), "logs");

    public static bool HasSampleLogs =>
        Directory.Exists(LogsDir) && Directory.EnumerateFiles(LogsDir, "output_log_*.txt").Any();

    private static Dictionary<string, ParseResult> ParseAll()
    {
        var results = new Dictionary<string, ParseResult>();
        foreach (var path in Directory.GetFiles(LogsDir, "output_log_*.txt"))
        {
            using var reader = new StreamReader(path);
            results[Path.GetFileName(path)] = LogParser.Parse(reader);
        }
        return results;
    }

    [Fact]
    public void ParsesAllSampleLogs()
    {
        if (!HasSampleLogs)
            return; // no local logs — nothing to verify

        var results = ParseAll();
        Assert.NotEmpty(results);

        // Every file's last visit is cut off at EOF (VRChat killed / still running).
        Assert.All(results.Values, r => Assert.False(r.Visits[^1].EndedCleanly));

        // Local user is identified in every file and is the same account throughout.
        Assert.All(results.Values, r =>
        {
            Assert.StartsWith("usr_", r.LocalUserId);
            Assert.False(string.IsNullOrEmpty(r.LocalUserName));
        });
        Assert.Single(results.Values.Select(r => r.LocalUserId).Distinct());

        // Every visit has a world id and the local user among its encounters.
        Assert.All(results.Values, r => Assert.All(r.Visits, v =>
        {
            Assert.StartsWith("wrld_", v.WorldId);
            Assert.Contains(v.Encounters, e => e.UserId == r.LocalUserId);
        }));

        // No Error-level lines leak into raw events.
        Assert.All(results.Values, r => Assert.DoesNotContain(r.RawEvents, e => e.Level == "Error"));
    }
}
