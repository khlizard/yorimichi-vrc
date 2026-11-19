//! Read-side queries for the UI. Port of `QueryService.cs` (identical SQL and parameter semantics).
//!
//! `QueryService` borrows a `&Connection`, so an app keeping one query connection in a
//! `Mutex<Connection>` can do `QueryService::new(&guard).get_visits(...)` per command.
//! Date filters are inclusive whole days: `from` starts at 00:00:00 of that day and `to`
//! runs through the end of that day.

use chrono::{Duration, NaiveDate, NaiveTime};
use rusqlite::{Connection, Row, ToSql};

use super::db;
use super::dtos::*;
use crate::Result;

/// Default page size of [`QueryService::search_players`] in the C# app.
pub const DEFAULT_PLAYER_LIMIT: i64 = 200;
/// Default (and maximum used by the UI) row limit of the 2000-row searches in the C# app.
pub const DEFAULT_RAW_LIMIT: i64 = 2000;

type Param<'a> = (&'a str, &'a dyn ToSql);

/// Prepares `sql` and binds only the named parameters it actually uses
/// (Dapper ignored extra properties; rusqlite would reject them).
fn query_rows<T>(
    conn: &Connection,
    sql: &str,
    params: &[Param<'_>],
    mut map: impl FnMut(&Row<'_>) -> rusqlite::Result<T>,
) -> rusqlite::Result<Vec<T>> {
    let mut stmt = conn.prepare(sql)?;
    for (name, value) in params {
        if let Some(idx) = stmt.parameter_index(name)? {
            stmt.raw_bind_parameter(idx, value)?;
        }
    }
    let mut rows = stmt.raw_query();
    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        out.push(map(row)?);
    }
    Ok(out)
}

fn date_filter(column: &str, from: Option<NaiveDate>, to: Option<NaiveDate>) -> (String, String, String) {
    (
        format!(" AND {column} >= @from AND {column} < @to"),
        from.map(|d| db::ts(d.and_time(NaiveTime::MIN))).unwrap_or_else(|| "0000".into()),
        to.map(|d| db::ts((d + Duration::days(1)).and_time(NaiveTime::MIN)))
            .unwrap_or_else(|| "9999".into()),
    )
}

fn like_pattern(term: &str) -> String {
    format!("%{}%", db::escape_like(term))
}

fn is_blank(s: &str) -> bool {
    s.trim().is_empty()
}

fn player_hit(r: &Row<'_>) -> rusqlite::Result<PlayerHit> {
    let last_seen: Option<String> = r.get(6)?;
    Ok(PlayerHit {
        id: r.get(0)?,
        user_id: r.get(1)?,
        display_name: r.get(2)?,
        is_local: r.get(3)?,
        other_names: r.get(4)?,
        encounter_count: r.get(5)?,
        last_seen_date: last_seen_date(last_seen.as_deref()),
        last_seen,
    })
}

pub struct QueryService<'a> {
    conn: &'a Connection,
}

impl<'a> QueryService<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    /// Players (excluding the local user) by most recent encounter, paged with OFFSET.
    pub fn get_recent_players(
        &self,
        from: Option<NaiveDate>,
        to: Option<NaiveDate>,
        offset: i64,
        limit: i64,
    ) -> Result<Vec<PlayerHit>> {
        let (clause, f, t) = date_filter("e.join_time", from, to);
        let sql = format!(
            r"
            SELECT p.id, p.user_id AS UserId, p.display_name AS DisplayName, p.is_local AS IsLocal,
                   (SELECT group_concat(pn.display_name, ', ') FROM player_names pn
                     WHERE pn.player_id = p.id AND pn.display_name <> p.display_name) AS OtherNames,
                   count(*) AS EncounterCount, max(e.join_time) AS LastSeen
            FROM encounters e JOIN players p ON p.id = e.player_id
            WHERE p.is_local = 0{clause}
            GROUP BY p.id
            ORDER BY LastSeen DESC, p.id DESC
            LIMIT @limit OFFSET @offset
            "
        );
        Ok(query_rows(
            self.conn,
            &sql,
            &[("@from", &f), ("@to", &t), ("@limit", &limit), ("@offset", &offset)],
            player_hit,
        )?)
    }

    /// Search every known name of every player (includes the local user), most encounters first.
    /// The C# defaults were `offset = 0`, `limit = 200` ([`DEFAULT_PLAYER_LIMIT`]).
    pub fn search_players(&self, term: &str, offset: i64, limit: i64) -> Result<Vec<PlayerHit>> {
        let pattern = like_pattern(term);
        Ok(query_rows(
            self.conn,
            r"
            SELECT p.id, p.user_id AS UserId, p.display_name AS DisplayName, p.is_local AS IsLocal,
                   (SELECT group_concat(pn.display_name, ', ') FROM player_names pn
                     WHERE pn.player_id = p.id AND pn.display_name <> p.display_name) AS OtherNames,
                   (SELECT count(*) FROM encounters e WHERE e.player_id = p.id) AS EncounterCount,
                   (SELECT max(e.join_time) FROM encounters e WHERE e.player_id = p.id) AS LastSeen
            FROM players p
            WHERE p.id IN (SELECT player_id FROM player_names WHERE display_name LIKE @pattern ESCAPE '\')
            ORDER BY EncounterCount DESC, p.id
            LIMIT @limit OFFSET @offset
            ",
            &[("@pattern", &pattern), ("@limit", &limit), ("@offset", &offset)],
            player_hit,
        )?)
    }

    pub fn get_encounters(
        &self,
        player_id: i64,
        from: Option<NaiveDate>,
        to: Option<NaiveDate>,
    ) -> Result<Vec<EncounterRow>> {
        let (clause, f, t) = date_filter("e.join_time", from, to);
        let sql = format!(
            r"
            SELECT v.id AS VisitId, e.display_name AS DisplayName, v.world_name AS WorldName,
                   v.instance_id AS InstanceId, e.join_time AS JoinTime, e.leave_time AS LeaveTime,
                   e.ended_cleanly AS EndedCleanly,
                   (SELECT a.avatar_name FROM avatar_events a WHERE a.visit_id = v.id AND a.display_name = e.display_name
                     ORDER BY a.event_time DESC, a.id DESC LIMIT 1) AS AvatarName,
                   (SELECT a.size_mb FROM avatar_events a WHERE a.visit_id = v.id AND a.display_name = e.display_name AND a.size_mb IS NOT NULL
                     ORDER BY a.event_time DESC, a.id DESC LIMIT 1) AS AvatarSizeMb
            FROM encounters e JOIN visits v ON v.id = e.visit_id
            WHERE e.player_id = @playerId{clause}
            ORDER BY e.join_time DESC
            "
        );
        Ok(query_rows(
            self.conn,
            &sql,
            &[("@playerId", &player_id), ("@from", &f), ("@to", &t)],
            |r| {
                let join_time: String = r.get(4)?;
                let leave_time: String = r.get(5)?;
                Ok(EncounterRow {
                    visit_id: r.get(0)?,
                    display_name: r.get(1)?,
                    world_name: r.get(2)?,
                    instance_id: r.get(3)?,
                    duration: format_duration(&join_time, &leave_time),
                    join_time,
                    leave_time,
                    ended_cleanly: r.get(6)?,
                    avatar_name: r.get(7)?,
                    avatar_size_mb: r.get(8)?,
                })
            },
        )?)
    }

    pub fn get_visits(
        &self,
        world_term: Option<&str>,
        from: Option<NaiveDate>,
        to: Option<NaiveDate>,
    ) -> Result<Vec<VisitRow>> {
        let (clause, f, t) = date_filter("v.join_time", from, to);
        let world_clause = if is_blank(world_term.unwrap_or("")) {
            ""
        } else {
            r" AND v.world_name LIKE @pattern ESCAPE '\'"
        };
        let sql = format!(
            r"
            SELECT v.id, v.world_name AS WorldName, v.instance_id AS InstanceId, v.location AS Location,
                   v.join_time AS JoinTime, v.leave_time AS LeaveTime, v.ended_cleanly AS EndedCleanly,
                   (SELECT count(*) FROM encounters e WHERE e.visit_id = v.id) AS PlayerCount
            FROM visits v
            WHERE 1=1{clause}{world_clause}
            ORDER BY v.join_time DESC
            "
        );
        let pattern = like_pattern(world_term.unwrap_or(""));
        Ok(query_rows(
            self.conn,
            &sql,
            &[("@from", &f), ("@to", &t), ("@pattern", &pattern)],
            |r| {
                let join_time: String = r.get(4)?;
                let leave_time: String = r.get(5)?;
                Ok(VisitRow {
                    id: r.get(0)?,
                    world_name: r.get(1)?,
                    instance_id: r.get(2)?,
                    location: r.get(3)?,
                    duration: format_duration(&join_time, &leave_time),
                    join_time,
                    leave_time,
                    ended_cleanly: r.get(6)?,
                    player_count: r.get(7)?,
                })
            },
        )?)
    }

    pub fn get_visit_players(&self, visit_id: i64) -> Result<Vec<VisitPlayerRow>> {
        Ok(query_rows(
            self.conn,
            r"
            SELECT e.player_id AS PlayerId, e.display_name AS DisplayName, p.is_local AS IsLocal,
                   e.join_time AS JoinTime, e.leave_time AS LeaveTime, e.ended_cleanly AS EndedCleanly,
                   (SELECT a.avatar_name FROM avatar_events a WHERE a.visit_id = v.id AND a.display_name = e.display_name
                     ORDER BY a.event_time DESC, a.id DESC LIMIT 1) AS AvatarName,
                   (SELECT a.size_mb FROM avatar_events a WHERE a.visit_id = v.id AND a.display_name = e.display_name AND a.size_mb IS NOT NULL
                     ORDER BY a.event_time DESC, a.id DESC LIMIT 1) AS AvatarSizeMb
            FROM encounters e JOIN players p ON p.id = e.player_id JOIN visits v ON v.id = e.visit_id
            WHERE e.visit_id = @visitId
            ORDER BY e.join_time
            ",
            &[("@visitId", &visit_id)],
            |r| {
                let join_time: String = r.get(3)?;
                let leave_time: String = r.get(4)?;
                Ok(VisitPlayerRow {
                    player_id: r.get(0)?,
                    display_name: r.get(1)?,
                    is_local: r.get(2)?,
                    duration: format_duration(&join_time, &leave_time),
                    join_time,
                    leave_time,
                    ended_cleanly: r.get(5)?,
                    avatar_name: r.get(6)?,
                    avatar_size_mb: r.get(7)?,
                })
            },
        )?)
    }

    pub fn get_visit_urls(&self, visit_id: i64) -> Result<Vec<UrlHit>> {
        Ok(query_rows(
            self.conn,
            r"
            SELECT u.visit_id AS VisitId, u.event_time AS EventTime, u.kind AS Kind, u.url AS Url, '' AS WorldName
            FROM url_events u WHERE u.visit_id = @visitId ORDER BY u.event_time
            ",
            &[("@visitId", &visit_id)],
            url_hit,
        )?)
    }

    /// `kind` of `None` or `Some("")` means all kinds. Limited to 2000 rows.
    pub fn search_urls(
        &self,
        term: &str,
        kind: Option<&str>,
        from: Option<NaiveDate>,
        to: Option<NaiveDate>,
    ) -> Result<Vec<UrlHit>> {
        let (clause, f, t) = date_filter("u.event_time", from, to);
        let kind_clause = if kind.is_none_or(str::is_empty) { "" } else { " AND u.kind = @kind" };
        let term_clause = if is_blank(term) { "" } else { r" AND u.url LIKE @pattern ESCAPE '\'" };
        let sql = format!(
            r"
            SELECT u.visit_id AS VisitId, u.event_time AS EventTime, u.kind AS Kind, u.url AS Url,
                   coalesce(v.world_name, '') AS WorldName
            FROM url_events u LEFT JOIN visits v ON v.id = u.visit_id
            WHERE 1=1{term_clause}{kind_clause}{clause}
            ORDER BY u.event_time DESC
            LIMIT 2000
            "
        );
        let pattern = like_pattern(term);
        Ok(query_rows(
            self.conn,
            &sql,
            &[("@pattern", &pattern), ("@kind", &kind), ("@from", &f), ("@to", &t)],
            url_hit,
        )?)
    }

    /// Raw log search, newest first with keyset paging on `(event_time DESC, id DESC)`.
    ///
    /// * `before`: `(event_time, id)` of the last row of the previous page; `None` for the first page.
    /// * `limit`: page size (the C# default was 2000, [`DEFAULT_RAW_LIMIT`]).
    /// * A non-blank `term` tries FTS5 first (each space-separated word as a quoted phrase) and
    ///   falls back to LIKE if FTS rejects the query. A blank term lists rows without a text filter.
    pub fn search_raw_events(
        &self,
        term: &str,
        level: Option<&str>,
        from: Option<NaiveDate>,
        to: Option<NaiveDate>,
        before: Option<(String, i64)>,
        limit: i64,
    ) -> Result<Vec<RawEventHit>> {
        let (clause, f, t) = date_filter("r.event_time", from, to);
        let level_clause = if level.is_none_or(str::is_empty) { "" } else { " AND r.level = @level" };
        let before_clause = if before.is_none() {
            ""
        } else {
            " AND (r.event_time < @bt OR (r.event_time = @bt AND r.id < @bid))"
        };
        let query = fts_query(term);
        let pattern = like_pattern(term);
        let bt: Option<&str> = before.as_ref().map(|b| b.0.as_str());
        let bid: i64 = before.as_ref().map(|b| b.1).unwrap_or(0);
        let params: [Param<'_>; 8] = [
            ("@query", &query),
            ("@pattern", &pattern),
            ("@level", &level),
            ("@from", &f),
            ("@to", &t),
            ("@bt", &bt),
            ("@bid", &bid),
            ("@limit", &limit),
        ];

        if !is_blank(term) {
            let sql = format!(
                r"
                SELECT r.id AS Id, r.visit_id AS VisitId, r.event_time AS EventTime, r.level AS Level,
                       r.message AS Message, coalesce(v.world_name, '') AS WorldName
                FROM raw_events_fts fts
                JOIN raw_events r ON r.id = fts.rowid
                LEFT JOIN visits v ON v.id = r.visit_id
                WHERE raw_events_fts MATCH @query{level_clause}{clause}{before_clause}
                ORDER BY r.event_time DESC, r.id DESC
                LIMIT @limit
                "
            );
            if let Ok(rows) = query_rows(self.conn, &sql, &params, raw_hit) {
                return Ok(rows);
            }
            // FTS query syntax the tokenizer can't handle (e.g. only symbols) - fall back to LIKE.
        }

        let term_clause = if is_blank(term) { "" } else { r" AND r.message LIKE @pattern ESCAPE '\'" };
        let sql = format!(
            r"
            SELECT r.id AS Id, r.visit_id AS VisitId, r.event_time AS EventTime, r.level AS Level,
                   r.message AS Message, coalesce(v.world_name, '') AS WorldName
            FROM raw_events r LEFT JOIN visits v ON v.id = r.visit_id
            WHERE 1=1{term_clause}{level_clause}{clause}{before_clause}
            ORDER BY r.event_time DESC, r.id DESC
            LIMIT @limit
            "
        );
        Ok(query_rows(self.conn, &sql, &params, raw_hit)?)
    }

    /// Avatar sightings by avatar or player name. Limited to 2000 rows.
    pub fn search_avatars(
        &self,
        term: &str,
        from: Option<NaiveDate>,
        to: Option<NaiveDate>,
    ) -> Result<Vec<AvatarRow>> {
        let (clause, f, t) = date_filter("a.event_time", from, to);
        let term_clause = if is_blank(term) {
            ""
        } else {
            r" AND (a.avatar_name LIKE @pattern ESCAPE '\' OR a.display_name LIKE @pattern ESCAPE '\')"
        };
        let sql = format!(
            r"
            SELECT a.visit_id AS VisitId, a.event_time AS EventTime, a.display_name AS DisplayName,
                   a.avatar_name AS AvatarName, a.size_mb AS SizeMb, a.load_seconds AS LoadSeconds,
                   coalesce(v.world_name, '') AS WorldName
            FROM avatar_events a LEFT JOIN visits v ON v.id = a.visit_id
            WHERE 1=1{term_clause}{clause}
            ORDER BY a.event_time DESC
            LIMIT 2000
            "
        );
        let pattern = like_pattern(term);
        Ok(query_rows(
            self.conn,
            &sql,
            &[("@pattern", &pattern), ("@from", &f), ("@to", &t)],
            |r| {
                Ok(AvatarRow {
                    visit_id: r.get(0)?,
                    event_time: r.get(1)?,
                    display_name: r.get(2)?,
                    avatar_name: r.get(3)?,
                    size_mb: r.get(4)?,
                    load_seconds: r.get(5)?,
                    world_name: r.get(6)?,
                })
            },
        )?)
    }

    /// `kind` of `None` or `Some("")` means all kinds. Limited to 2000 rows.
    pub fn search_notifications(
        &self,
        term: &str,
        kind: Option<&str>,
        from: Option<NaiveDate>,
        to: Option<NaiveDate>,
    ) -> Result<Vec<NotificationRow>> {
        let (clause, f, t) = date_filter("n.event_time", from, to);
        let kind_clause = if kind.is_none_or(str::is_empty) { "" } else { " AND n.kind = @kind" };
        let term_clause = if is_blank(term) {
            ""
        } else {
            r" AND (n.sender_name LIKE @pattern ESCAPE '\' OR n.message LIKE @pattern ESCAPE '\' OR n.details LIKE @pattern ESCAPE '\')"
        };
        let sql = format!(
            r"
            SELECT n.id, n.notification_id AS NotificationId, n.event_time AS EventTime, n.kind AS Kind,
                   n.sender_user_id AS SenderUserId, n.sender_name AS SenderName, n.message AS Message,
                   n.details AS Details, n.accepted_at AS AcceptedAt
            FROM notifications n
            WHERE 1=1{term_clause}{kind_clause}{clause}
            ORDER BY n.event_time DESC
            LIMIT 2000
            "
        );
        let pattern = like_pattern(term);
        Ok(query_rows(
            self.conn,
            &sql,
            &[("@pattern", &pattern), ("@kind", &kind), ("@from", &f), ("@to", &t)],
            |r| {
                let accepted_at: Option<String> = r.get(8)?;
                Ok(NotificationRow {
                    id: r.get(0)?,
                    notification_id: r.get(1)?,
                    event_time: r.get(2)?,
                    kind: r.get(3)?,
                    sender_user_id: r.get(4)?,
                    sender_name: r.get(5)?,
                    message: r.get(6)?,
                    details: r.get(7)?,
                    response: if accepted_at.is_none() { String::new() } else { "応答済".to_string() },
                    accepted_at,
                })
            },
        )?)
    }

    pub fn get_notification_kinds(&self) -> Result<Vec<String>> {
        Ok(query_rows(
            self.conn,
            "SELECT DISTINCT kind FROM notifications ORDER BY kind",
            &[],
            |r| r.get(0),
        )?)
    }

    /// Mute/block/vote-kick events by target name. Limited to 2000 rows.
    pub fn search_moderation_events(
        &self,
        term: &str,
        from: Option<NaiveDate>,
        to: Option<NaiveDate>,
    ) -> Result<Vec<ModerationRow>> {
        let (clause, f, t) = date_filter("m.event_time", from, to);
        let term_clause = if is_blank(term) { "" } else { r" AND m.target_name LIKE @pattern ESCAPE '\'" };
        let sql = format!(
            r"
            SELECT m.visit_id AS VisitId, m.event_time AS EventTime, m.kind AS Kind, m.target_name AS TargetName,
                   coalesce(v.world_name, '') AS WorldName
            FROM moderation_events m LEFT JOIN visits v ON v.id = m.visit_id
            WHERE 1=1{term_clause}{clause}
            ORDER BY m.event_time DESC
            LIMIT 2000
            "
        );
        let pattern = like_pattern(term);
        Ok(query_rows(
            self.conn,
            &sql,
            &[("@pattern", &pattern), ("@from", &f), ("@to", &t)],
            |r| {
                Ok(ModerationRow {
                    visit_id: r.get(0)?,
                    event_time: r.get(1)?,
                    kind: r.get(2)?,
                    target_name: r.get(3)?,
                    world_name: r.get(4)?,
                })
            },
        )?)
    }
}

fn url_hit(r: &Row<'_>) -> rusqlite::Result<UrlHit> {
    Ok(UrlHit {
        visit_id: r.get(0)?,
        event_time: r.get(1)?,
        kind: r.get(2)?,
        url: r.get(3)?,
        world_name: r.get(4)?,
    })
}

fn raw_hit(r: &Row<'_>) -> rusqlite::Result<RawEventHit> {
    Ok(RawEventHit {
        id: r.get(0)?,
        visit_id: r.get(1)?,
        event_time: r.get(2)?,
        level: r.get(3)?,
        message: r.get(4)?,
        world_name: r.get(5)?,
    })
}

/// Wraps each space-separated word in quotes so FTS treats them as literal phrases.
fn fts_query(term: &str) -> String {
    term.split(' ')
        .filter(|w| !w.is_empty())
        .map(|w| format!("\"{}\"", w.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(" ")
}
