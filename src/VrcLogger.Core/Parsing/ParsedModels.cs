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

public sealed record ParseResult(
    string? LocalUserId,
    string? LocalUserName,
    List<ParsedVisit> Visits,
    List<ParsedUrlEvent> OrphanUrlEvents,
    List<ParsedRawEvent> RawEvents,
    List<ParsedNotification> Notifications,
    List<(string NotificationId, DateTime Time)> AnsweredNotifications,
    List<ParsedModerationEvent> ModerationEvents,
    DateTime? LastTimestamp);

/// <summary>Options controlling what the parser keeps.</summary>
public sealed record ParseOptions(IReadOnlyList<string> RawNoisePatterns)
{
    public static readonly ParseOptions Default = new(RawLogNoise.Default);
}
