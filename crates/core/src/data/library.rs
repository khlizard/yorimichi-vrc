//! User-written data (player memo/tags, world notes, albums) and the activity views built on
//! the log (day timeline, calendar activity, "on this day", statistics).
//!
//! Everything user-written is keyed by `players.id` / `worlds.id` / `photos.id`, which survive a
//! log re-import (`visits.id` does not).
//!
//! A *logical day* starts at `day_start_hour:00` and ends at the same hour the next day; a
//! timestamp `t` belongs to `date(t, '-{h} hours')`. A visit belongs to the day of its `join_time`.

use std::collections::{BTreeMap, HashSet};

use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime};
use rusqlite::{params, Connection, OptionalExtension};

use super::db;
use super::dtos::{format_duration, PlayerHit};
use super::library_dtos::*;
use super::photo_query::{photo_row, select_photo, PhotoRow};
use super::query::{player_hit, query_rows, PLAYER_SEARCH_SELECT};
use crate::{Error, Result};

const DEFAULT_TAG_COLOR: &str = "#6b7280";
/// A single visit/encounter never counts for more than this (broken log rows).
const MAX_SECONDS: i64 = 24 * 3600;
const IN_CHUNK: usize = 500;
const TS_FORMAT: &str = "%Y-%m-%d %H:%M:%S";

/// SQL expression: seconds between two timestamp columns, clamped to `0..=24h`.
fn secs_sql(join: &str, leave: &str) -> String {
    format!(
        "min({MAX_SECONDS}, max(0, coalesce(CAST(strftime('%s', {leave}) AS INTEGER) - CAST(strftime('%s', {join}) AS INTEGER), 0)))"
    )
}

fn hour(h: u32) -> u32 {
    h.min(23)
}

fn day_expr(column: &str, h: u32) -> String {
    format!("date({column}, '-{} hours')", hour(h))
}

/// `[start, end)` of a logical day as stored timestamps.
fn window(day: NaiveDate, h: u32) -> (String, String) {
    let start = day.and_hms_opt(hour(h), 0, 0).expect("hour is clamped to 0..=23");
    (db::ts(start), db::ts(start + Duration::days(1)))
}

/// Optional inclusive day bounds as `[from, to)` strings usable in comparisons.
fn bounds(from: Option<NaiveDate>, to: Option<NaiveDate>, h: u32) -> (String, String) {
    let start = from.map(|d| window(d, h).0).unwrap_or_else(|| "0000".into());
    let end = to.map(|d| window(d, h).1).unwrap_or_else(|| "9999".into());
    (start, end)
}

fn valid_color(color: &str) -> String {
    let c = color.trim();
    let ok = c.len() == 7 && c.starts_with('#') && c[1..].chars().all(|ch| ch.is_ascii_hexdigit());
    if ok {
        c.to_ascii_lowercase()
    } else {
        DEFAULT_TAG_COLOR.to_string()
    }
}

fn non_empty(kind: &str, s: &str) -> Result<String> {
    let t = s.trim();
    if t.is_empty() {
        return Err(Error::Invalid(format!("{kind} name must not be empty")));
    }
    Ok(t.to_string())
}

fn parse_ts(s: &str) -> Option<NaiveDateTime> {
    NaiveDateTime::parse_from_str(s, TS_FORMAT).ok()
}

pub struct LibraryService<'a> {
    conn: &'a Connection,
}

impl<'a> LibraryService<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    // ---- tags -------------------------------------------------------------------------

    pub fn list_tags(&self) -> Result<Vec<TagCount>> {
        Ok(query_rows(
            self.conn,
            r"SELECT t.id, t.name, t.color, (SELECT count(*) FROM player_tags pt WHERE pt.tag_id = t.id)
              FROM tags t ORDER BY t.name, t.id",
            &[],
            |r| {
                Ok(TagCount {
                    tag: Tag { id: r.get(0)?, name: r.get(1)?, color: r.get(2)? },
                    player_count: r.get(3)?,
                })
            },
        )?)
    }

    /// Creates a tag; if a tag with that (trimmed) name exists it is returned unchanged.
    pub fn create_tag(&self, name: &str, color: &str) -> Result<Tag> {
        let name = non_empty("tag", name)?;
        let color = valid_color(color);
        self.conn
            .execute("INSERT OR IGNORE INTO tags(name, color) VALUES (?1, ?2)", params![name, color])?;
        Ok(self.conn.query_row("SELECT id, name, color FROM tags WHERE name = ?1", [&name], tag_row)?)
    }

    pub fn update_tag(&self, id: i64, name: &str, color: &str) -> Result<()> {
        let name = non_empty("tag", name)?;
        self.conn.execute(
            "UPDATE tags SET name = ?1, color = ?2 WHERE id = ?3",
            params![name, valid_color(color), id],
        )?;
        Ok(())
    }

    pub fn delete_tag(&self, id: i64) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute("DELETE FROM player_tags WHERE tag_id = ?1", [id])?;
        tx.execute("DELETE FROM tags WHERE id = ?1", [id])?;
        tx.commit()?;
        Ok(())
    }

    // ---- player memo / tags / profile --------------------------------------------------

    /// Replaces the player's tag set (unknown tag ids are ignored).
    pub fn set_player_tags(&self, player_id: i64, tag_ids: &[i64]) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute("DELETE FROM player_tags WHERE player_id = ?1", [player_id])?;
        for tag_id in tag_ids {
            tx.execute(
                "INSERT OR IGNORE INTO player_tags(player_id, tag_id) SELECT ?1, id FROM tags WHERE id = ?2",
                params![player_id, tag_id],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Stores the trimmed memo; an empty memo removes the row.
    pub fn set_player_memo(&self, player_id: i64, memo: &str, now: NaiveDateTime) -> Result<()> {
        let memo = memo.trim();
        if memo.is_empty() {
            self.conn.execute("DELETE FROM player_notes WHERE player_id = ?1", [player_id])?;
        } else {
            self.conn.execute(
                r"INSERT INTO player_notes(player_id, memo, updated_at) VALUES (?1, ?2, ?3)
                  ON CONFLICT(player_id) DO UPDATE SET memo = excluded.memo, updated_at = excluded.updated_at",
                params![player_id, memo, db::ts(now)],
            )?;
        }
        Ok(())
    }

    fn tags_of(&self, player_id: i64) -> Result<Vec<Tag>> {
        Ok(query_rows(
            self.conn,
            r"SELECT t.id, t.name, t.color FROM player_tags pt JOIN tags t ON t.id = pt.tag_id
              WHERE pt.player_id = @p ORDER BY t.name, t.id",
            &[("@p", &player_id)],
            tag_row,
        )?)
    }

    /// `total_seconds` sums encounter durations (each clamped to 24 h); `photo_count` counts
    /// non-missing photos taken while the player was present in the photo's linked visit.
    pub fn player_profile(&self, player_id: i64) -> Result<Option<PlayerProfile>> {
        let secs = secs_sql("e.join_time", "e.leave_time");
        let sql = format!(
            r"SELECT p.id, p.user_id, p.display_name, p.is_local, coalesce(n.memo, ''),
                     (SELECT min(e.join_time) FROM encounters e WHERE e.player_id = p.id),
                     (SELECT max(e.join_time) FROM encounters e WHERE e.player_id = p.id),
                     (SELECT count(*) FROM encounters e WHERE e.player_id = p.id),
                     (SELECT coalesce(sum({secs}), 0) FROM encounters e WHERE e.player_id = p.id),
                     (SELECT count(*) FROM photos ph
                       WHERE ph.missing = 0 AND ph.visit_id IS NOT NULL
                         AND EXISTS (SELECT 1 FROM encounters e WHERE e.visit_id = ph.visit_id AND e.player_id = p.id
                                       AND e.join_time <= ph.taken_at AND e.leave_time >= ph.taken_at))
              FROM players p LEFT JOIN player_notes n ON n.player_id = p.id
              WHERE p.id = @id"
        );
        let Some(mut profile) = query_rows(self.conn, &sql, &[("@id", &player_id)], |r| {
            Ok(PlayerProfile {
                id: r.get(0)?,
                user_id: r.get(1)?,
                display_name: r.get(2)?,
                is_local: r.get(3)?,
                memo: r.get(4)?,
                tags: Vec::new(),
                first_met: r.get(5)?,
                last_met: r.get(6)?,
                encounter_count: r.get(7)?,
                total_seconds: r.get(8)?,
                photo_count: r.get(9)?,
            })
        })?
        .into_iter()
        .next() else {
            return Ok(None);
        };
        profile.tags = self.tags_of(player_id)?;
        Ok(Some(profile))
    }

    /// Players carrying the tag, same shape and ordering as `QueryService::search_players`.
    pub fn players_by_tag(&self, tag_id: i64) -> Result<Vec<PlayerHit>> {
        let sql = format!(
            r"{PLAYER_SEARCH_SELECT}
            WHERE p.id IN (SELECT player_id FROM player_tags WHERE tag_id = @tag)
            ORDER BY EncounterCount DESC, p.id"
        );
        Ok(query_rows(self.conn, &sql, &[("@tag", &tag_id)], player_hit)?)
    }

    /// Memo/tag marks for list decoration, ascending by player id. Unmarked players are omitted.
    pub fn player_marks(&self, player_ids: &[i64]) -> Result<Vec<PlayerMark>> {
        let mut marks: BTreeMap<i64, PlayerMark> = BTreeMap::new();
        for chunk in player_ids.chunks(IN_CHUNK) {
            let marks_in = vec!["?"; chunk.len()].join(",");
            let mut stmt = self
                .conn
                .prepare(&format!("SELECT player_id FROM player_notes WHERE player_id IN ({marks_in})"))?;
            let mut rows = stmt.query(rusqlite::params_from_iter(chunk.iter()))?;
            while let Some(row) = rows.next()? {
                let id: i64 = row.get(0)?;
                marks.entry(id).or_insert_with(|| PlayerMark { player_id: id, has_memo: false, tags: Vec::new() }).has_memo =
                    true;
            }
            let mut stmt = self.conn.prepare(&format!(
                r"SELECT pt.player_id, t.id, t.name, t.color FROM player_tags pt JOIN tags t ON t.id = pt.tag_id
                  WHERE pt.player_id IN ({marks_in}) ORDER BY t.name, t.id"
            ))?;
            let mut rows = stmt.query(rusqlite::params_from_iter(chunk.iter()))?;
            while let Some(row) = rows.next()? {
                let id: i64 = row.get(0)?;
                let tag = Tag { id: row.get(1)?, name: row.get(2)?, color: row.get(3)? };
                marks.entry(id).or_insert_with(|| PlayerMark { player_id: id, has_memo: false, tags: Vec::new() }).tags.push(tag);
            }
        }
        Ok(marks.into_values().collect())
    }

    // ---- badges ------------------------------------------------------------------------

    /// Non-local players of one visit with first-met / reunion information, by first join time.
    fn visit_people(&self, visit_id: i64, visit_join: &str, reunion_days: i32) -> Result<Vec<TimelinePlayer>> {
        let jt = parse_ts(visit_join);
        let rows = query_rows(
            self.conn,
            r"SELECT e.player_id, p.user_id, p.display_name, min(e.join_time), max(e.leave_time),
                     NOT EXISTS (SELECT 1 FROM encounters x WHERE x.player_id = e.player_id
                                   AND x.visit_id <> @v AND x.join_time < @jt),
                     (SELECT max(x.leave_time) FROM encounters x WHERE x.player_id = e.player_id
                        AND x.visit_id <> @v AND x.leave_time < @jt),
                     EXISTS (SELECT 1 FROM player_notes n WHERE n.player_id = e.player_id)
              FROM encounters e JOIN players p ON p.id = e.player_id
              WHERE e.visit_id = @v AND p.is_local = 0
              GROUP BY e.player_id
              ORDER BY min(e.join_time), e.player_id",
            &[("@v", &visit_id), ("@jt", &visit_join)],
            |r| {
                let prev: Option<String> = r.get(6)?;
                Ok((
                    TimelinePlayer {
                        player_id: r.get(0)?,
                        user_id: r.get(1)?,
                        display_name: r.get(2)?,
                        join_time: r.get(3)?,
                        leave_time: r.get(4)?,
                        first_met: r.get(5)?,
                        days_since_last: None,
                        has_memo: r.get(7)?,
                    },
                    prev,
                ))
            },
        )?;
        Ok(rows
            .into_iter()
            .map(|(mut p, prev)| {
                if reunion_days > 0 {
                    if let (Some(jt), Some(prev)) = (jt, prev.as_deref().and_then(parse_ts)) {
                        let gap = (jt - prev).num_days();
                        if gap >= i64::from(reunion_days) {
                            p.days_since_last = Some(gap);
                        }
                    }
                }
                p
            })
            .collect())
    }

    /// Badges for the non-local players of a visit; only players that have a badge are returned.
    /// `days_since_last` is set only when `reunion_days > 0` and the gap reaches it.
    pub fn visit_player_badges(&self, visit_id: i64, reunion_days: i32) -> Result<Vec<PlayerBadge>> {
        let join: Option<String> = self
            .conn
            .query_row("SELECT join_time FROM visits WHERE id = ?1", [visit_id], |r| r.get(0))
            .optional()?;
        let Some(join) = join else { return Ok(Vec::new()) };
        Ok(self
            .visit_people(visit_id, &join, reunion_days)?
            .into_iter()
            .filter(|p| p.first_met || p.days_since_last.is_some())
            .map(|p| PlayerBadge { player_id: p.player_id, first_met: p.first_met, days_since_last: p.days_since_last })
            .collect())
    }

    // ---- worlds ------------------------------------------------------------------------

    fn world_select() -> String {
        let secs = secs_sql("join_time", "leave_time");
        let world_ref = super::photo_query::WORLD_REF;
        let joins = super::photo_query::FROM_JOINS;
        format!(
            r"SELECT w.id, w.world_id, w.name,
                     coalesce(vs.cnt, 0), vs.first_visit, vs.last_visit, coalesce(vs.secs, 0),
                     coalesce(n.memo, ''), coalesce(n.favorite, 0), coalesce(pc.cnt, 0)
              FROM worlds w
              LEFT JOIN (SELECT world_id, count(*) AS cnt, min(join_time) AS first_visit,
                                max(join_time) AS last_visit, sum({secs}) AS secs
                         FROM visits GROUP BY world_id) vs ON vs.world_id = w.id
              LEFT JOIN world_notes n ON n.world_id = w.id
              LEFT JOIN (SELECT ref, count(*) AS cnt
                         FROM (SELECT {world_ref} AS ref {joins} WHERE p.missing = 0)
                         WHERE ref IS NOT NULL GROUP BY ref) pc ON pc.ref = w.id"
        )
    }

    fn world_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<WorldRow> {
        Ok(WorldRow {
            id: r.get(0)?,
            world_id: r.get(1)?,
            name: r.get(2)?,
            visit_count: r.get(3)?,
            first_visit: r.get(4)?,
            last_visit: r.get(5)?,
            total_seconds: r.get(6)?,
            memo: r.get(7)?,
            favorite: r.get(8)?,
            photo_count: r.get(9)?,
        })
    }

    /// Worlds with visit statistics. `term` matches the name or the memo (LIKE).
    pub fn worlds(&self, term: Option<&str>, favorite_only: bool, sort: WorldSort) -> Result<Vec<WorldRow>> {
        let term = term.map(str::trim).filter(|t| !t.is_empty());
        let mut clauses = Vec::new();
        if term.is_some() {
            clauses.push(r"(w.name LIKE @pattern ESCAPE '\' OR n.memo LIKE @pattern ESCAPE '\')");
        }
        if favorite_only {
            clauses.push("n.favorite = 1");
        }
        let where_clause = if clauses.is_empty() { String::new() } else { format!(" WHERE {}", clauses.join(" AND ")) };
        let order = match sort {
            WorldSort::LastVisit => "vs.last_visit DESC, w.id DESC",
            WorldSort::VisitCount => "coalesce(vs.cnt, 0) DESC, vs.last_visit DESC, w.id DESC",
            WorldSort::TotalTime => "coalesce(vs.secs, 0) DESC, w.id DESC",
            WorldSort::Name => "w.name COLLATE NOCASE, w.id",
        };
        let sql = format!("{}{where_clause} ORDER BY {order}", Self::world_select());
        let pattern = format!("%{}%", db::escape_like(term.unwrap_or("")));
        Ok(query_rows(self.conn, &sql, &[("@pattern", &pattern)], Self::world_row)?)
    }

    pub fn world(&self, id: i64) -> Result<Option<WorldRow>> {
        let sql = format!("{} WHERE w.id = @id", Self::world_select());
        Ok(query_rows(self.conn, &sql, &[("@id", &id)], Self::world_row)?.into_iter().next())
    }

    /// Upserts the world's memo and favorite flag; the row is removed when both are empty.
    pub fn set_world_note(&self, world_ref_id: i64, memo: &str, favorite: bool, now: NaiveDateTime) -> Result<()> {
        let memo = memo.trim();
        if memo.is_empty() && !favorite {
            self.conn.execute("DELETE FROM world_notes WHERE world_id = ?1", [world_ref_id])?;
        } else {
            self.conn.execute(
                r"INSERT INTO world_notes(world_id, memo, favorite, updated_at) VALUES (?1, ?2, ?3, ?4)
                  ON CONFLICT(world_id) DO UPDATE SET memo = excluded.memo, favorite = excluded.favorite,
                                                      updated_at = excluded.updated_at",
                params![world_ref_id, memo, favorite, db::ts(now)],
            )?;
        }
        Ok(())
    }

    // ---- timeline ----------------------------------------------------------------------

    /// One logical day. `show_first_met = false` forces every `first_met` to false; reunion
    /// badges follow `reunion_days` (0 disables them). Always returns a day, possibly empty.
    pub fn timeline_day(
        &self,
        day: NaiveDate,
        day_start_hour: u32,
        reunion_days: i32,
        show_first_met: bool,
    ) -> Result<TimelineDay> {
        let (start, end) = window(day, day_start_hour);
        let secs = secs_sql("v.join_time", "v.leave_time");
        let sql = format!(
            r"SELECT v.id, v.world_name, w.world_id, v.world_id, coalesce(n.favorite, 0), coalesce(n.memo, ''),
                     v.instance_id, v.location, v.join_time, v.leave_time, {secs}
              FROM visits v LEFT JOIN worlds w ON w.id = v.world_id LEFT JOIN world_notes n ON n.world_id = v.world_id
              WHERE v.join_time >= @s AND v.join_time < @e
              ORDER BY v.join_time, v.id"
        );
        let mut visits = query_rows(self.conn, &sql, &[("@s", &start), ("@e", &end)], |r| {
            let join_time: String = r.get(8)?;
            let leave_time: String = r.get(9)?;
            Ok(TimelineVisit {
                visit_id: r.get(0)?,
                world_name: r.get(1)?,
                world_id: r.get(2)?,
                world_ref_id: r.get(3)?,
                world_favorite: r.get(4)?,
                world_memo: r.get(5)?,
                instance_id: r.get(6)?,
                location: r.get(7)?,
                duration: format_duration(&join_time, &leave_time),
                join_time,
                leave_time,
                seconds: r.get(10)?,
                players: Vec::new(),
                photos: Vec::new(),
            })
        })?;

        let mut people: HashSet<i64> = HashSet::new();
        for v in &mut visits {
            v.players = self.visit_people(v.visit_id, &v.join_time, reunion_days)?;
            if !show_first_met {
                for p in &mut v.players {
                    p.first_met = false;
                }
            }
            people.extend(v.players.iter().map(|p| p.player_id));
        }

        // A photo belongs to the day its timestamp falls in: either to a visit of this day it is
        // linked to, or (no visit / visit of another day) to the loose list.
        let mut loose_photos = Vec::new();
        for photo in self.window_photos(&start, &end)? {
            match photo.visit_id.and_then(|id| visits.iter_mut().find(|v| v.visit_id == id)) {
                Some(v) => v.photos.push(photo),
                None => loose_photos.push(photo),
            }
        }

        let photo_count = (loose_photos.len() + visits.iter().map(|v| v.photos.len()).sum::<usize>()) as i64;
        Ok(TimelineDay {
            day: day.format("%Y-%m-%d").to_string(),
            total_seconds: visits.iter().map(|v| v.seconds).sum(),
            visit_count: visits.len() as i64,
            people_count: people.len() as i64,
            photo_count,
            visits,
            loose_photos,
            start,
            end,
        })
    }

    /// Non-missing photos taken in `[start, end)`, ascending.
    fn window_photos(&self, start: &str, end: &str) -> Result<Vec<PhotoRow>> {
        let sql = format!(
            "{} WHERE p.missing = 0 AND p.taken_at >= @s AND p.taken_at < @e ORDER BY p.taken_at, p.id",
            select_photo()
        );
        Ok(query_rows(self.conn, &sql, &[("@s", &start), ("@e", &end)], photo_row)?)
    }

    /// Per-day totals in `[from, to]` (logical days, inclusive); only days with a visit or a
    /// non-missing photo, ascending. Visit time is attributed to the day of `join_time`.
    pub fn activity(&self, from: NaiveDate, to: NaiveDate, day_start_hour: u32) -> Result<Vec<DayActivity>> {
        let (start, _) = window(from, day_start_hour);
        let (_, end) = window(to, day_start_hour);
        let params: &[(&str, &dyn rusqlite::ToSql)] = &[("@s", &start), ("@e", &end)];
        let mut days: BTreeMap<String, DayActivity> = BTreeMap::new();
        let d = day_expr("v.join_time", day_start_hour);
        let secs = secs_sql("v.join_time", "v.leave_time");
        let visit_rows = query_rows(
            self.conn,
            &format!(
                "SELECT {d}, sum({secs}), count(*) FROM visits v WHERE v.join_time >= @s AND v.join_time < @e GROUP BY 1"
            ),
            params,
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?, r.get::<_, i64>(2)?)),
        )?;
        let people_rows = query_rows(
            self.conn,
            &format!(
                r"SELECT {d}, count(DISTINCT e.player_id)
                  FROM encounters e JOIN visits v ON v.id = e.visit_id JOIN players p ON p.id = e.player_id
                  WHERE p.is_local = 0 AND v.join_time >= @s AND v.join_time < @e GROUP BY 1"
            ),
            params,
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)),
        )?;
        let photo_rows = query_rows(
            self.conn,
            &format!(
                "SELECT {}, count(*) FROM photos WHERE missing = 0 AND taken_at >= @s AND taken_at < @e GROUP BY 1",
                day_expr("taken_at", day_start_hour)
            ),
            params,
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)),
        )?;

        for (day, seconds, count) in visit_rows {
            days.entry(day.clone())
                .or_insert_with(|| empty_day(day))
                .apply_visits(seconds, count);
        }
        for (day, n) in people_rows {
            days.entry(day.clone()).or_insert_with(|| empty_day(day)).people_count = n;
        }
        for (day, n) in photo_rows {
            days.entry(day.clone()).or_insert_with(|| empty_day(day)).photo_count = n;
        }
        Ok(days.into_values().collect())
    }
}

fn empty_day(day: String) -> DayActivity {
    DayActivity { day, seconds: 0, visit_count: 0, people_count: 0, photo_count: 0 }
}

impl DayActivity {
    fn apply_visits(&mut self, seconds: i64, count: i64) {
        self.seconds = seconds;
        self.visit_count = count;
    }
}

fn tag_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Tag> {
    Ok(Tag { id: r.get(0)?, name: r.get(1)?, color: r.get(2)? })
}

impl LibraryService<'_> {
    /// Nearest logical day after (`forward`) or before `day` that has a visit or a non-missing photo.
    pub fn adjacent_active_day(&self, day: NaiveDate, day_start_hour: u32, forward: bool) -> Result<Option<String>> {
        let (start, end) = window(day, day_start_hour);
        let d = hour(day_start_hour);
        let sql = if forward {
            format!(
                r"SELECT date(min(t), '-{d} hours') FROM (
                    SELECT min(join_time) AS t FROM visits WHERE join_time >= @b
                    UNION ALL SELECT min(taken_at) FROM photos WHERE missing = 0 AND taken_at >= @b)"
            )
        } else {
            format!(
                r"SELECT date(max(t), '-{d} hours') FROM (
                    SELECT max(join_time) AS t FROM visits WHERE join_time < @b
                    UNION ALL SELECT max(taken_at) FROM photos WHERE missing = 0 AND taken_at < @b)"
            )
        };
        let bound = if forward { &end } else { &start };
        Ok(self.conn.query_row(&sql, rusqlite::named_params! {"@b": bound}, |r| r.get(0))?)
    }

    /// The most recent logical day with a visit or a non-missing photo.
    pub fn latest_active_day(&self, day_start_hour: u32) -> Result<Option<String>> {
        Ok(self.conn.query_row(
            &format!(
                r"SELECT date(max(t), '-{} hours') FROM (
                    SELECT max(join_time) AS t FROM visits
                    UNION ALL SELECT max(taken_at) FROM photos WHERE missing = 0)",
                hour(day_start_hour)
            ),
            [],
            |r| r.get(0),
        )?)
    }

    /// Calendar years that have visits or non-missing photos, newest first.
    pub fn years(&self) -> Result<Vec<i32>> {
        Ok(query_rows(
            self.conn,
            r"SELECT y FROM (
                SELECT CAST(substr(join_time, 1, 4) AS INTEGER) AS y FROM visits
                UNION SELECT CAST(substr(taken_at, 1, 4) AS INTEGER) FROM photos WHERE missing = 0)
              WHERE y > 0 ORDER BY y DESC",
            &[],
            |r| r.get(0),
        )?)
    }

    /// The same month/day in each earlier year with activity, newest year first.
    /// Feb 29 only matches leap years.
    pub fn on_this_day(&self, day: NaiveDate, day_start_hour: u32) -> Result<Vec<MemoryYear>> {
        let Some(&oldest) = self.years()?.last() else { return Ok(Vec::new()) };
        let mut out = Vec::new();
        // One year below the oldest calendar year: a logical day can start on Dec 31.
        for year in ((oldest - 1)..day.year()).rev() {
            let Some(d) = NaiveDate::from_ymd_opt(year, day.month(), day.day()) else { continue };
            if let Some(m) = self.memory_year(d, day_start_hour)? {
                out.push(m);
            }
        }
        Ok(out)
    }

    fn memory_year(&self, day: NaiveDate, h: u32) -> Result<Option<MemoryYear>> {
        let (start, end) = window(day, h);
        let names: Vec<String> = query_rows(
            self.conn,
            "SELECT world_name FROM visits WHERE join_time >= @s AND join_time < @e ORDER BY join_time, id",
            &[("@s", &start), ("@e", &end)],
            |r| r.get(0),
        )?;
        let photos = self.window_photos(&start, &end)?;
        if names.is_empty() && photos.is_empty() {
            return Ok(None);
        }
        let people: i64 = self.conn.query_row(
            r"SELECT count(DISTINCT e.player_id)
              FROM encounters e JOIN visits v ON v.id = e.visit_id JOIN players p ON p.id = e.player_id
              WHERE p.is_local = 0 AND v.join_time >= ?1 AND v.join_time < ?2",
            [&start, &end],
            |r| r.get(0),
        )?;
        let mut worlds: Vec<String> = Vec::new();
        for n in &names {
            if worlds.len() < 5 && !worlds.contains(n) {
                worlds.push(n.clone());
            }
        }
        Ok(Some(MemoryYear {
            year: day.year(),
            day: day.format("%Y-%m-%d").to_string(),
            visit_count: names.len() as i64,
            people_count: people,
            photo_count: photos.len() as i64,
            worlds,
            photos: pick_memory_photos(photos, 8),
        }))
    }

    // ---- stats -------------------------------------------------------------------------

    /// Totals over logical days `from..=to` (open ends allowed); top lists by seconds, non-local only.
    pub fn stats(
        &self,
        from: Option<NaiveDate>,
        to: Option<NaiveDate>,
        day_start_hour: u32,
        top_n: usize,
    ) -> Result<StatsSummary> {
        let (start, end) = bounds(from, to, day_start_hour);
        let limit = top_n as i64;
        let p: &[(&str, &dyn rusqlite::ToSql)] = &[("@s", &start), ("@e", &end), ("@n", &limit)];
        let vsecs = secs_sql("v.join_time", "v.leave_time");
        let esecs = secs_sql("e.join_time", "e.leave_time");
        let in_v = "v.join_time >= @s AND v.join_time < @e";

        let (total_seconds, visit_count, world_count): (i64, i64, i64) = query_rows(
            self.conn,
            &format!("SELECT coalesce(sum({vsecs}), 0), count(*), count(DISTINCT v.world_id) FROM visits v WHERE {in_v}"),
            p,
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?[0];
        let people_count: i64 = query_rows(
            self.conn,
            &format!(
                r"SELECT count(DISTINCT e.player_id)
                  FROM encounters e JOIN visits v ON v.id = e.visit_id JOIN players pl ON pl.id = e.player_id
                  WHERE pl.is_local = 0 AND {in_v}"
            ),
            p,
            |r| r.get(0),
        )?[0];
        let photo_count: i64 = query_rows(
            self.conn,
            "SELECT count(*) FROM photos WHERE missing = 0 AND taken_at >= @s AND taken_at < @e",
            p,
            |r| r.get(0),
        )?[0];
        let active_days: i64 = query_rows(
            self.conn,
            &format!(
                r"SELECT count(*) FROM (
                    SELECT {} AS d FROM visits v WHERE {in_v}
                    UNION SELECT {} FROM photos WHERE missing = 0 AND taken_at >= @s AND taken_at < @e)",
                day_expr("v.join_time", day_start_hour),
                day_expr("taken_at", day_start_hour)
            ),
            p,
            |r| r.get(0),
        )?[0];
        let top_players = query_rows(
            self.conn,
            &format!(
                r"SELECT pl.id, pl.user_id, pl.display_name, sum({esecs}) AS secs, count(DISTINCT e.visit_id)
                  FROM encounters e JOIN visits v ON v.id = e.visit_id JOIN players pl ON pl.id = e.player_id
                  WHERE pl.is_local = 0 AND {in_v}
                  GROUP BY pl.id ORDER BY secs DESC, pl.id LIMIT @n"
            ),
            p,
            |r| {
                Ok(TopPlayer {
                    player_id: r.get(0)?,
                    user_id: r.get(1)?,
                    display_name: r.get(2)?,
                    seconds: r.get(3)?,
                    count: r.get(4)?,
                })
            },
        )?;
        let top_worlds = query_rows(
            self.conn,
            &format!(
                r"SELECT v.world_id, coalesce(w.name, v.world_name), sum({vsecs}) AS secs, count(*)
                  FROM visits v LEFT JOIN worlds w ON w.id = v.world_id
                  WHERE {in_v}
                  GROUP BY coalesce(CAST(v.world_id AS TEXT), 'n:' || v.world_name)
                  ORDER BY secs DESC, 1 LIMIT @n"
            ),
            p,
            |r| Ok(TopWorld { world_ref_id: r.get(0)?, name: r.get(1)?, seconds: r.get(2)?, count: r.get(3)? }),
        )?;
        Ok(StatsSummary {
            total_seconds,
            visit_count,
            world_count,
            people_count,
            photo_count,
            active_days,
            top_players,
            top_worlds,
        })
    }

    // ---- albums ------------------------------------------------------------------------

    fn album_select() -> &'static str {
        r"SELECT a.id, a.name, a.description,
                 CASE WHEN a.cover_photo_id IS NOT NULL AND EXISTS (
                        SELECT 1 FROM album_photos ap JOIN photos p ON p.id = ap.photo_id
                        WHERE ap.album_id = a.id AND ap.photo_id = a.cover_photo_id AND p.missing = 0)
                      THEN a.cover_photo_id
                      ELSE (SELECT ap.photo_id FROM album_photos ap JOIN photos p ON p.id = ap.photo_id
                            WHERE ap.album_id = a.id AND p.missing = 0 ORDER BY ap.position, ap.photo_id LIMIT 1)
                 END,
                 (SELECT count(*) FROM album_photos ap WHERE ap.album_id = a.id),
                 a.created_at,
                 (SELECT min(p.taken_at) FROM album_photos ap JOIN photos p ON p.id = ap.photo_id WHERE ap.album_id = a.id),
                 (SELECT max(p.taken_at) FROM album_photos ap JOIN photos p ON p.id = ap.photo_id WHERE ap.album_id = a.id)
          FROM albums a"
    }

    fn album_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<AlbumRow> {
        Ok(AlbumRow {
            id: r.get(0)?,
            name: r.get(1)?,
            description: r.get(2)?,
            cover_photo_id: r.get(3)?,
            photo_count: r.get(4)?,
            created_at: r.get(5)?,
            first_taken: r.get(6)?,
            last_taken: r.get(7)?,
        })
    }

    /// Newest created first.
    pub fn albums(&self) -> Result<Vec<AlbumRow>> {
        let sql = format!("{} ORDER BY a.created_at DESC, a.id DESC", Self::album_select());
        Ok(query_rows(self.conn, &sql, &[], Self::album_row)?)
    }

    pub fn album(&self, id: i64) -> Result<Option<AlbumRow>> {
        let sql = format!("{} WHERE a.id = @id", Self::album_select());
        Ok(query_rows(self.conn, &sql, &[("@id", &id)], Self::album_row)?.into_iter().next())
    }

    pub fn create_album(&self, name: &str, description: &str, now: NaiveDateTime) -> Result<AlbumRow> {
        let name = non_empty("album", name)?;
        self.conn.execute(
            "INSERT INTO albums(name, description, created_at) VALUES (?1, ?2, ?3)",
            params![name, description.trim(), db::ts(now)],
        )?;
        let id = self.conn.last_insert_rowid();
        Ok(self.album(id)?.expect("album was just inserted"))
    }

    pub fn update_album(&self, id: i64, name: &str, description: &str, cover_photo_id: Option<i64>) -> Result<()> {
        let name = non_empty("album", name)?;
        self.conn.execute(
            "UPDATE albums SET name = ?1, description = ?2, cover_photo_id = ?3 WHERE id = ?4",
            params![name, description.trim(), cover_photo_id, id],
        )?;
        Ok(())
    }

    pub fn delete_album(&self, id: i64) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute("DELETE FROM album_photos WHERE album_id = ?1", [id])?;
        tx.execute("DELETE FROM albums WHERE id = ?1", [id])?;
        tx.commit()?;
        Ok(())
    }

    /// Appends photos in the given order; photos already in the album and unknown ids are skipped.
    /// Returns the number added.
    pub fn add_photos(&self, album_id: i64, photo_ids: &[i64]) -> Result<usize> {
        let tx = self.conn.unchecked_transaction()?;
        let mut next: i64 = tx.query_row(
            "SELECT coalesce(max(position), -1) + 1 FROM album_photos WHERE album_id = ?1",
            [album_id],
            |r| r.get(0),
        )?;
        let mut added = 0;
        for photo_id in photo_ids {
            let n = tx.execute(
                "INSERT OR IGNORE INTO album_photos(album_id, photo_id, position) SELECT ?1, id, ?2 FROM photos WHERE id = ?3",
                params![album_id, next, photo_id],
            )?;
            if n > 0 {
                next += 1;
                added += n;
            }
        }
        tx.commit()?;
        Ok(added)
    }

    pub fn remove_photos(&self, album_id: i64, photo_ids: &[i64]) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        for photo_id in photo_ids {
            tx.execute("DELETE FROM album_photos WHERE album_id = ?1 AND photo_id = ?2", params![album_id, photo_id])?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Rewrites positions: listed photos first in the given order, then the rest in their old order.
    pub fn reorder(&self, album_id: i64, photo_ids_in_order: &[i64]) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        let current: Vec<i64> = query_rows(
            &tx,
            "SELECT photo_id FROM album_photos WHERE album_id = @a ORDER BY position, photo_id",
            &[("@a", &album_id)],
            |r| r.get(0),
        )?;
        let members: HashSet<i64> = current.iter().copied().collect();
        let mut seen: HashSet<i64> = HashSet::new();
        let mut order: Vec<i64> = Vec::with_capacity(current.len());
        for id in photo_ids_in_order {
            if members.contains(id) && seen.insert(*id) {
                order.push(*id);
            }
        }
        order.extend(current.iter().filter(|id| !seen.contains(id)));
        for (pos, id) in order.iter().enumerate() {
            tx.execute(
                "UPDATE album_photos SET position = ?1 WHERE album_id = ?2 AND photo_id = ?3",
                params![pos as i64, album_id, id],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Photos in album order, missing ones included (the UI shows a placeholder).
    pub fn album_photos(&self, album_id: i64) -> Result<Vec<PhotoRow>> {
        let sql = format!(
            "{} JOIN album_photos ap ON ap.photo_id = p.id WHERE ap.album_id = @a ORDER BY ap.position, p.id",
            select_photo()
        );
        Ok(query_rows(self.conn, &sql, &[("@a", &album_id)], photo_row)?)
    }

    pub fn albums_of_photo(&self, photo_id: i64) -> Result<Vec<i64>> {
        Ok(query_rows(
            self.conn,
            "SELECT album_id FROM album_photos WHERE photo_id = @p ORDER BY album_id",
            &[("@p", &photo_id)],
            |r| r.get(0),
        )?)
    }
}

/// Favorites first (time order), then the others spread evenly over the day, up to `max` in total.
fn pick_memory_photos(photos: Vec<PhotoRow>, max: usize) -> Vec<PhotoRow> {
    let (mut picked, rest): (Vec<PhotoRow>, Vec<PhotoRow>) = photos.into_iter().partition(|p| p.favorite);
    picked.truncate(max);
    let slots = max - picked.len();
    if slots > 0 && !rest.is_empty() {
        let n = rest.len();
        if n <= slots {
            picked.extend(rest);
        } else {
            let keep: HashSet<usize> = (0..slots).map(|i| i * n / slots).collect();
            picked.extend(rest.into_iter().enumerate().filter(|(i, _)| keep.contains(i)).map(|(_, p)| p));
        }
    }
    picked
}
