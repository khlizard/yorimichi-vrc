namespace VrcLogger.Core.Data;

public sealed class PlayerHit
{
    public long Id { get; set; }
    public string UserId { get; set; } = "";
    public string DisplayName { get; set; } = "";
    public bool IsLocal { get; set; }
    public string? OtherNames { get; set; }
    public int EncounterCount { get; set; }
}

public sealed class EncounterRow
{
    public long VisitId { get; set; }
    public string DisplayName { get; set; } = "";
    public string WorldName { get; set; } = "";
    public string InstanceId { get; set; } = "";
    public string JoinTime { get; set; } = "";
    public string LeaveTime { get; set; } = "";
    public bool EndedCleanly { get; set; }
    public string Duration => Dtos.FormatDuration(JoinTime, LeaveTime);
}

public sealed class VisitRow
{
    public long Id { get; set; }
    public string WorldName { get; set; } = "";
    public string InstanceId { get; set; } = "";
    public string Location { get; set; } = "";
    public string JoinTime { get; set; } = "";
    public string LeaveTime { get; set; } = "";
    public bool EndedCleanly { get; set; }
    public int PlayerCount { get; set; }
    public string Duration => Dtos.FormatDuration(JoinTime, LeaveTime);
}

public sealed class VisitPlayerRow
{
    public long PlayerId { get; set; }
    public string DisplayName { get; set; } = "";
    public bool IsLocal { get; set; }
    public string JoinTime { get; set; } = "";
    public string LeaveTime { get; set; } = "";
    public bool EndedCleanly { get; set; }
    public string Duration => Dtos.FormatDuration(JoinTime, LeaveTime);
}

public sealed class UrlHit
{
    public long? VisitId { get; set; }
    public string EventTime { get; set; } = "";
    public string Kind { get; set; } = "";
    public string Url { get; set; } = "";
    public string WorldName { get; set; } = "";
}

public sealed class RawEventHit
{
    public long? VisitId { get; set; }
    public string EventTime { get; set; } = "";
    public string Level { get; set; } = "";
    public string Message { get; set; } = "";
    public string WorldName { get; set; } = "";
}

public sealed class NotificationRow
{
    public long Id { get; set; }
    public string NotificationId { get; set; } = "";
    public string EventTime { get; set; } = "";
    public string Kind { get; set; } = "";
    public string SenderUserId { get; set; } = "";
    public string SenderName { get; set; } = "";
    public string Message { get; set; } = "";
    public string Details { get; set; } = "";
    public string? AcceptedAt { get; set; }
    public string Response => AcceptedAt is null ? "" : "応答済";
}

public sealed class ModerationRow
{
    public long? VisitId { get; set; }
    public string EventTime { get; set; } = "";
    public string Kind { get; set; } = "";
    public string TargetName { get; set; } = "";
    public string WorldName { get; set; } = "";
}

public static class Dtos
{
    public static string FormatDuration(string join, string leave)
    {
        if (!DateTime.TryParse(join, out var j) || !DateTime.TryParse(leave, out var l) || l < j)
            return "";
        var d = l - j;
        return d.TotalHours >= 1 ? $"{(int)d.TotalHours}h {d.Minutes:D2}m" : $"{d.Minutes}m {d.Seconds:D2}s";
    }
}
