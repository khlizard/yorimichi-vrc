namespace VrcLogger.Core.Parsing;

public sealed record ParsedEncounter(
    string UserId,
    string DisplayName,
    DateTime JoinTime,
    DateTime LeaveTime,
    bool EndedCleanly);

public sealed record ParsedUrlEvent(
    string Kind, // "video" | "string" | "image"
    string Url,
    DateTime Time);

public sealed record ParsedRawEvent(
    DateTime Time,
    string Level, // "Debug" | "Warning" | ...
    string Message,
    int? VisitIndex); // index into ParseResult.Visits, null if outside any visit

public sealed record ParsedVisit(
    string? WorldId,
    string WorldName,
    string InstanceId,
    string Location,
    DateTime JoinTime,
    DateTime LeaveTime,
    bool EndedCleanly,
    List<ParsedEncounter> Encounters,
    List<ParsedUrlEvent> UrlEvents);

public sealed record ParsedNotification(
    string NotificationId, // not_… / frq_…
    string Kind,           // invite | requestInvite | friendRequest | boop | message | …
    string SenderUserId,
    string SenderName,
    string Message,        // first line only
    string Details,        // raw "k=v, k=v" text from the details block
    string CreatedAtUtc,   // as written in the log, e.g. "06/23/2026 12:40:24"
    DateTime Time);        // log line timestamp (local)

public sealed record ParsedModerationEvent(
    string Kind,           // mute | unmute | block | unblock | hide | unhide | votekick | …
    string TargetName,
    DateTime Time,
    int? VisitIndex);

/// <summary>A player (identified by display name only) wearing an avatar; size is known only when it was downloaded.</summary>
public sealed class ParsedAvatarEvent(string playerName, string avatarName, DateTime time, int? visitIndex)
{
    public string PlayerName { get; } = playerName;
    public string AvatarName { get; } = avatarName;
    public DateTime Time { get; } = time;
    public int? VisitIndex { get; } = visitIndex;
    public double? SizeMb { get; set; }
    public int? LoadSeconds { get; set; }
}

public sealed record ParseResult(
    string? LocalUserId,
    string? LocalUserName,
    List<ParsedVisit> Visits,
    List<ParsedUrlEvent> OrphanUrlEvents,
    List<ParsedRawEvent> RawEvents,
    List<ParsedNotification> Notifications,
    List<(string NotificationId, DateTime Time)> AnsweredNotifications,
    List<ParsedModerationEvent> ModerationEvents,
    List<ParsedAvatarEvent> AvatarEvents,
    DateTime? LastTimestamp);

/// <summary>Options controlling what the parser keeps.</summary>
public sealed record ParseOptions(IReadOnlyList<string> RawNoisePatterns)
{
    public static readonly ParseOptions Default = new(RawLogNoise.Default);
}
