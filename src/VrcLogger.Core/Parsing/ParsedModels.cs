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

public sealed record ParseResult(
    string? LocalUserId,
    string? LocalUserName,
    List<ParsedVisit> Visits,
    List<ParsedUrlEvent> OrphanUrlEvents,
    List<ParsedRawEvent> RawEvents,
    DateTime? LastTimestamp);
