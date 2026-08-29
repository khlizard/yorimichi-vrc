using System.Globalization;

namespace VrcLogger.Core.Parsing;

public static class LogParser
{
    private static readonly TimeSpan UrlDedupeWindow = TimeSpan.FromSeconds(10);

    public static ParseResult Parse(TextReader reader)
    {
        string? localUserId = null;
        string? localUserName = null;
        var visits = new List<VisitBuilder>();
        var orphanUrls = new List<ParsedUrlEvent>();
        var orphanUrlLastSeen = new Dictionary<(string Kind, string Url), DateTime>();
        var rawEvents = new List<ParsedRawEvent>();
        VisitBuilder? current = null;
        string? pendingWorldName = null;
        DateTime? lastTimestamp = null;

        while (reader.ReadLine() is { } line)
        {
            var prefix = LogLineRegex.LinePrefix().Match(line);
            if (!prefix.Success)
                continue; // continuation line (stack trace etc.)

            var ts = DateTime.ParseExact(prefix.Groups[1].Value, "yyyy.MM.dd HH:mm:ss", CultureInfo.InvariantCulture);
            var level = prefix.Groups[2].Value;
            var message = prefix.Groups[3].Value;
            lastTimestamp = ts;

            // Handle visit transitions before recording the raw event so the
            // "Joining wrld_..." line is attributed to the visit it starts.
            var joining = LogLineRegex.JoiningWorld().Match(message);
            if (joining.Success)
            {
                current?.Close(ts, endedCleanly: false); // missed OnLeftRoom
                current = new VisitBuilder(
                    worldId: joining.Groups[1].Value,
                    worldName: pendingWorldName ?? "",
                    instanceId: joining.Groups[2].Value,
                    location: $"{joining.Groups[1].Value}:{joining.Groups[2].Value}",
                    joinTime: ts);
                visits.Add(current);
                pendingWorldName = null;
            }

            if (level != "Error")
                rawEvents.Add(new ParsedRawEvent(ts, level, message, current is null ? null : visits.Count - 1));

            if (joining.Success)
                continue;

            var m = LogLineRegex.PlayerJoined().Match(message);
            if (m.Success)
            {
                current?.OpenEncounter(m.Groups[2].Value, m.Groups[1].Value, ts);
                continue;
            }

            m = LogLineRegex.PlayerLeft().Match(message);
            if (m.Success)
            {
                current?.CloseEncounter(m.Groups[2].Value, ts, endedCleanly: true);
                continue;
            }

            m = LogLineRegex.EnteringRoom().Match(message);
            if (m.Success)
            {
                pendingWorldName = m.Groups[1].Value;
                continue;
            }

            if (LogLineRegex.LeftRoom().IsMatch(message))
            {
                current?.Close(ts, endedCleanly: true);
                current = null;
                continue;
            }

            m = LogLineRegex.Authenticated().Match(message);
            if (m.Success)
            {
                localUserName = m.Groups[1].Value;
                localUserId = m.Groups[2].Value;
                continue;
            }

            var kind =
                LogLineRegex.VideoUrl().Match(message) is { Success: true } vm ? ("video", vm.Groups[1].Value) :
                LogLineRegex.StringUrl().Match(message) is { Success: true } sm ? ("string", sm.Groups[1].Value) :
                LogLineRegex.ImageUrl().Match(message) is { Success: true } im ? ("image", im.Groups[1].Value) :
                default((string, string)?);
            if (kind is var (k, url) && k is not null)
            {
                if (current is not null)
                    current.AddUrl(k, url, ts, UrlDedupeWindow);
                else if (!orphanUrlLastSeen.TryGetValue((k, url), out var seen) || ts - seen > UrlDedupeWindow)
                {
                    orphanUrlLastSeen[(k, url)] = ts;
                    orphanUrls.Add(new ParsedUrlEvent(k, url, ts));
                }
            }
        }

        // Log ended abruptly (crash / VRChat still running): close everything at the last seen time.
        if (current is not null && lastTimestamp is { } eof)
            current.Close(eof, endedCleanly: false);

        return new ParseResult(
            localUserId, localUserName,
            visits.Select(v => v.Build()).ToList(),
            orphanUrls, rawEvents, lastTimestamp);
    }

    private sealed class VisitBuilder(string worldId, string worldName, string instanceId, string location, DateTime joinTime)
    {
        private readonly Dictionary<string, (string Name, DateTime JoinTime)> _open = [];
        private readonly List<ParsedEncounter> _encounters = [];
        private readonly List<ParsedUrlEvent> _urls = [];
        private readonly Dictionary<(string Kind, string Url), DateTime> _urlLastSeen = [];
        private DateTime _leaveTime;
        private bool _endedCleanly;
        private bool _closed;

        public void OpenEncounter(string userId, string name, DateTime ts)
        {
            if (!_open.ContainsKey(userId))
                _open[userId] = (name, ts);
        }

        public void CloseEncounter(string userId, DateTime ts, bool endedCleanly)
        {
            if (_open.Remove(userId, out var e))
                _encounters.Add(new ParsedEncounter(userId, e.Name, e.JoinTime, ts, endedCleanly));
        }

        public void AddUrl(string kind, string url, DateTime ts, TimeSpan dedupeWindow)
        {
            if (_urlLastSeen.TryGetValue((kind, url), out var seen) && ts - seen <= dedupeWindow)
            {
                _urlLastSeen[(kind, url)] = ts;
                return;
            }
            _urlLastSeen[(kind, url)] = ts;
            _urls.Add(new ParsedUrlEvent(kind, url, ts));
        }

        public void Close(DateTime ts, bool endedCleanly)
        {
            if (_closed)
                return;
            foreach (var userId in _open.Keys.ToList())
                CloseEncounter(userId, ts, endedCleanly);
            _leaveTime = ts;
            _endedCleanly = endedCleanly;
            _closed = true;
        }

        public ParsedVisit Build() => new(
            worldId, worldName, instanceId, location, joinTime,
            _closed ? _leaveTime : joinTime, _endedCleanly,
            _encounters.OrderBy(e => e.JoinTime).ToList(),
            _urls);
    }
}
