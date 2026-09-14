using Dapper;
using Microsoft.Data.Sqlite;

namespace VrcLogger.Core.Data;

public sealed class QueryService(SqliteConnection conn)
{
    private static (string Clause, string From, string To) DateFilter(string column, DateTime? from, DateTime? to) =>
        ($" AND {column} >= @from AND {column} < @to",
         from is { } f ? Db.Ts(f.Date) : "0000",
         to is { } t ? Db.Ts(t.Date.AddDays(1)) : "9999");

    public List<PlayerHit> SearchPlayers(string term)
    {
        return conn.Query<PlayerHit>("""
            SELECT p.id, p.user_id AS UserId, p.display_name AS DisplayName, p.is_local AS IsLocal,
                   (SELECT group_concat(pn.display_name, ', ') FROM player_names pn
                     WHERE pn.player_id = p.id AND pn.display_name <> p.display_name) AS OtherNames,
                   (SELECT count(*) FROM encounters e WHERE e.player_id = p.id) AS EncounterCount
            FROM players p
            WHERE p.id IN (SELECT player_id FROM player_names WHERE display_name LIKE @pattern ESCAPE '\')
            ORDER BY EncounterCount DESC
            LIMIT 200
            """, new { pattern = $"%{EscapeLike(term)}%" }).ToList();
    }

    public List<EncounterRow> GetEncounters(long playerId, DateTime? from, DateTime? to)
    {
        var (clause, f, t) = DateFilter("e.join_time", from, to);
        return conn.Query<EncounterRow>($"""
            SELECT v.id AS VisitId, e.display_name AS DisplayName, v.world_name AS WorldName,
                   v.instance_id AS InstanceId, e.join_time AS JoinTime, e.leave_time AS LeaveTime,
                   e.ended_cleanly AS EndedCleanly
            FROM encounters e JOIN visits v ON v.id = e.visit_id
            WHERE e.player_id = @playerId{clause}
            ORDER BY e.join_time DESC
            """, new { playerId, from = f, to = t }).ToList();
    }

    public List<VisitRow> GetVisits(string? worldTerm, DateTime? from, DateTime? to)
    {
        var (clause, f, t) = DateFilter("v.join_time", from, to);
        var worldClause = string.IsNullOrWhiteSpace(worldTerm) ? "" : " AND v.world_name LIKE @pattern ESCAPE '\\'";
        return conn.Query<VisitRow>($"""
            SELECT v.id, v.world_name AS WorldName, v.instance_id AS InstanceId, v.location AS Location,
                   v.join_time AS JoinTime, v.leave_time AS LeaveTime, v.ended_cleanly AS EndedCleanly,
                   (SELECT count(*) FROM encounters e WHERE e.visit_id = v.id) AS PlayerCount
            FROM visits v
            WHERE 1=1{clause}{worldClause}
            ORDER BY v.join_time DESC
            """, new { from = f, to = t, pattern = $"%{EscapeLike(worldTerm ?? "")}%" }).ToList();
    }

    public List<VisitPlayerRow> GetVisitPlayers(long visitId)
    {
        return conn.Query<VisitPlayerRow>("""
            SELECT e.player_id AS PlayerId, e.display_name AS DisplayName, p.is_local AS IsLocal,
                   e.join_time AS JoinTime, e.leave_time AS LeaveTime, e.ended_cleanly AS EndedCleanly
            FROM encounters e JOIN players p ON p.id = e.player_id
            WHERE e.visit_id = @visitId
            ORDER BY e.join_time
            """, new { visitId }).ToList();
    }

    public List<UrlHit> GetVisitUrls(long visitId)
    {
        return conn.Query<UrlHit>("""
            SELECT u.visit_id AS VisitId, u.event_time AS EventTime, u.kind AS Kind, u.url AS Url, '' AS WorldName
            FROM url_events u WHERE u.visit_id = @visitId ORDER BY u.event_time
            """, new { visitId }).ToList();
    }

    public List<UrlHit> SearchUrls(string term, string? kind, DateTime? from, DateTime? to)
    {
        var (clause, f, t) = DateFilter("u.event_time", from, to);
        var kindClause = string.IsNullOrEmpty(kind) ? "" : " AND u.kind = @kind";
        var termClause = string.IsNullOrWhiteSpace(term) ? "" : " AND u.url LIKE @pattern ESCAPE '\\'";
        return conn.Query<UrlHit>($"""
            SELECT u.visit_id AS VisitId, u.event_time AS EventTime, u.kind AS Kind, u.url AS Url,
                   coalesce(v.world_name, '') AS WorldName
            FROM url_events u LEFT JOIN visits v ON v.id = u.visit_id
            WHERE 1=1{termClause}{kindClause}{clause}
            ORDER BY u.event_time DESC
            LIMIT 2000
            """, new { pattern = $"%{EscapeLike(term)}%", kind, from = f, to = t }).ToList();
    }

    public List<RawEventHit> SearchRawEvents(string term, string? level, DateTime? from, DateTime? to)
    {
        var (clause, f, t) = DateFilter("r.event_time", from, to);
        var levelClause = string.IsNullOrEmpty(level) ? "" : " AND r.level = @level";
        var p = new { query = FtsQuery(term), pattern = $"%{EscapeLike(term)}%", level, from = f, to = t };

        if (!string.IsNullOrWhiteSpace(term))
        {
            try
            {
                return conn.Query<RawEventHit>($"""
                    SELECT r.visit_id AS VisitId, r.event_time AS EventTime, r.level AS Level,
                           r.message AS Message, coalesce(v.world_name, '') AS WorldName
                    FROM raw_events_fts fts
                    JOIN raw_events r ON r.id = fts.rowid
                    LEFT JOIN visits v ON v.id = r.visit_id
                    WHERE raw_events_fts MATCH @query{levelClause}{clause}
                    ORDER BY r.event_time DESC
                    LIMIT 2000
                    """, p).ToList();
            }
            catch (SqliteException)
            {
                // FTS query syntax the tokenizer can't handle (e.g. only symbols) — fall back to LIKE.
            }
        }

        var termClause = string.IsNullOrWhiteSpace(term) ? "" : " AND r.message LIKE @pattern ESCAPE '\\'";
        return conn.Query<RawEventHit>($"""
            SELECT r.visit_id AS VisitId, r.event_time AS EventTime, r.level AS Level,
                   r.message AS Message, coalesce(v.world_name, '') AS WorldName
            FROM raw_events r LEFT JOIN visits v ON v.id = r.visit_id
            WHERE 1=1{termClause}{levelClause}{clause}
            ORDER BY r.event_time DESC
            LIMIT 2000
            """, p).ToList();
    }

    public List<NotificationRow> SearchNotifications(string term, string? kind, DateTime? from, DateTime? to)
    {
        var (clause, f, t) = DateFilter("n.event_time", from, to);
        var kindClause = string.IsNullOrEmpty(kind) ? "" : " AND n.kind = @kind";
        var termClause = string.IsNullOrWhiteSpace(term) ? ""
            : " AND (n.sender_name LIKE @pattern ESCAPE '\\' OR n.message LIKE @pattern ESCAPE '\\' OR n.details LIKE @pattern ESCAPE '\\')";
        return conn.Query<NotificationRow>($"""
            SELECT n.id, n.notification_id AS NotificationId, n.event_time AS EventTime, n.kind AS Kind,
                   n.sender_user_id AS SenderUserId, n.sender_name AS SenderName, n.message AS Message,
                   n.details AS Details, n.accepted_at AS AcceptedAt
            FROM notifications n
            WHERE 1=1{termClause}{kindClause}{clause}
            ORDER BY n.event_time DESC
            LIMIT 2000
            """, new { pattern = $"%{EscapeLike(term)}%", kind, from = f, to = t }).ToList();
    }

    public List<string> GetNotificationKinds() =>
        conn.Query<string>("SELECT DISTINCT kind FROM notifications ORDER BY kind").ToList();

    public List<ModerationRow> SearchModerationEvents(string term, DateTime? from, DateTime? to)
    {
        var (clause, f, t) = DateFilter("m.event_time", from, to);
        var termClause = string.IsNullOrWhiteSpace(term) ? "" : " AND m.target_name LIKE @pattern ESCAPE '\\'";
        return conn.Query<ModerationRow>($"""
            SELECT m.visit_id AS VisitId, m.event_time AS EventTime, m.kind AS Kind, m.target_name AS TargetName,
                   coalesce(v.world_name, '') AS WorldName
            FROM moderation_events m LEFT JOIN visits v ON v.id = m.visit_id
            WHERE 1=1{termClause}{clause}
            ORDER BY m.event_time DESC
            LIMIT 2000
            """, new { pattern = $"%{EscapeLike(term)}%", from = f, to = t }).ToList();
    }

    /// <summary>Wraps each whitespace-separated term in quotes so FTS treats them as literal phrases.</summary>
    private static string FtsQuery(string term) =>
        string.Join(" ", term.Split(' ', StringSplitOptions.RemoveEmptyEntries)
            .Select(w => $"\"{w.Replace("\"", "\"\"")}\""));

    private static string EscapeLike(string s) => Db.EscapeLike(s);
}
