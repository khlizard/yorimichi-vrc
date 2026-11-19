//! Read/write queries for the photo library (`photos` table) and their DTOs.
//!
//! A photo's world is the linked visit's world; photos without a visit fall back to the
//! world recorded in their XMP metadata.

use chrono::NaiveDate;
use rusqlite::{Connection, Row};
use serde::Serialize;

use super::query::{date_filter, query_rows, Param};
use crate::Result;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PhotoRow {
    pub id: i64,
    pub path: String,
    pub file_name: String,
    pub taken_at: String,
    pub time_source: String,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub favorite: bool,
    pub missing: bool,
    pub online_only: bool,
    pub visit_id: Option<i64>,
    pub world_name: Option<String>,
    pub world_ref_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PhotoPlayer {
    pub player_id: i64,
    pub user_id: String,
    pub display_name: String,
    pub is_local: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PhotoDetail {
    pub photo: PhotoRow,
    pub world_id: Option<String>,
    pub instance_id: Option<String>,
    pub location: Option<String>,
    pub visit_join_time: Option<String>,
    pub visit_leave_time: Option<String>,
    pub author_id: Option<String>,
    pub players: Vec<PhotoPlayer>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DayCount {
    pub day: String,
    pub count: i64,
}

#[derive(Debug, Clone, Default)]
pub struct PhotoFilter {
    pub from: Option<NaiveDate>,
    pub to: Option<NaiveDate>,
    pub world_ref_id: Option<i64>,
    pub player_id: Option<i64>,
    pub visit_id: Option<i64>,
    pub favorite_only: bool,
    pub include_missing: bool,
}

pub(crate) const FROM_JOINS: &str = r"FROM photos p
    LEFT JOIN visits v ON v.id = p.visit_id
    LEFT JOIN worlds xw ON xw.world_id = p.xmp_world_id";

pub(crate) const WORLD_REF: &str = "CASE WHEN v.id IS NOT NULL THEN v.world_id ELSE xw.id END";

pub(crate) fn select_photo() -> String {
    format!(
        r"SELECT p.id, p.path, p.file_name, p.taken_at, p.time_source, p.width, p.height,
                 p.favorite, p.missing, p.online_only, p.visit_id,
                 CASE WHEN v.id IS NOT NULL THEN v.world_name ELSE p.xmp_world_name END,
                 {WORLD_REF}
          {FROM_JOINS}"
    )
}

pub(crate) fn photo_row(r: &Row<'_>) -> rusqlite::Result<PhotoRow> {
    Ok(PhotoRow {
        id: r.get(0)?,
        path: r.get(1)?,
        file_name: r.get(2)?,
        taken_at: r.get(3)?,
        time_source: r.get(4)?,
        width: r.get(5)?,
        height: r.get(6)?,
        favorite: r.get(7)?,
        missing: r.get(8)?,
        online_only: r.get(9)?,
        visit_id: r.get(10)?,
        world_name: r.get(11)?,
        world_ref_id: r.get(12)?,
    })
}

/// WHERE clause (starting with `WHERE 1=1`) plus the bound date strings for [`PhotoFilter`].
fn filter_clause(f: &PhotoFilter) -> (String, String, String) {
    let (date, from, to) = date_filter("p.taken_at", f.from, f.to);
    let mut sql = format!("WHERE 1=1{date}");
    if !f.include_missing {
        sql.push_str(" AND p.missing = 0");
    }
    if f.favorite_only {
        sql.push_str(" AND p.favorite = 1");
    }
    if f.world_ref_id.is_some() {
        sql.push_str(&format!(" AND ({WORLD_REF}) = @world"));
    }
    if f.visit_id.is_some() {
        sql.push_str(" AND p.visit_id = @visit");
    }
    if f.player_id.is_some() {
        sql.push_str(
            r" AND EXISTS (SELECT 1 FROM encounters e WHERE e.visit_id = p.visit_id AND e.player_id = @player
                           AND e.join_time <= p.taken_at AND e.leave_time >= p.taken_at)",
        );
    }
    (sql, from, to)
}

pub struct PhotoService<'a> {
    conn: &'a Connection,
}

impl<'a> PhotoService<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    /// Newest first with keyset paging on `(taken_at DESC, id DESC)`;
    /// `before` is `(taken_at, id)` of the last row of the previous page.
    pub fn photos(&self, filter: &PhotoFilter, before: Option<(String, i64)>, limit: usize) -> Result<Vec<PhotoRow>> {
        let (mut clause, from, to) = filter_clause(filter);
        if before.is_some() {
            clause.push_str(" AND (p.taken_at < @bt OR (p.taken_at = @bt AND p.id < @bid))");
        }
        let sql = format!("{} {clause} ORDER BY p.taken_at DESC, p.id DESC LIMIT @limit", select_photo());
        let bt: Option<&str> = before.as_ref().map(|b| b.0.as_str());
        let bid: i64 = before.as_ref().map(|b| b.1).unwrap_or(0);
        let limit = limit as i64;
        let params: [Param<'_>; 8] = [
            ("@from", &from),
            ("@to", &to),
            ("@world", &filter.world_ref_id),
            ("@visit", &filter.visit_id),
            ("@player", &filter.player_id),
            ("@bt", &bt),
            ("@bid", &bid),
            ("@limit", &limit),
        ];
        Ok(query_rows(self.conn, &sql, &params, photo_row)?)
    }

    /// Photo counts per logical day (a day starts at `day_start_hour`), newest first.
    pub fn photo_days(&self, filter: &PhotoFilter, day_start_hour: u32) -> Result<Vec<DayCount>> {
        let (clause, from, to) = filter_clause(filter);
        let sql = format!(
            "SELECT date(p.taken_at, '-{} hours') AS day, count(*) {FROM_JOINS} {clause} GROUP BY day ORDER BY day DESC",
            day_start_hour.min(23)
        );
        Ok(query_rows(
            self.conn,
            &sql,
            &[
                ("@from", &from),
                ("@to", &to),
                ("@world", &filter.world_ref_id),
                ("@visit", &filter.visit_id),
                ("@player", &filter.player_id),
            ],
            |r| Ok(DayCount { day: r.get(0)?, count: r.get(1)? }),
        )?)
    }

    /// Photos linked to a visit, oldest first.
    pub fn photos_for_visit(&self, visit_id: i64) -> Result<Vec<PhotoRow>> {
        let sql = format!("{} WHERE p.visit_id = @visit AND p.missing = 0 ORDER BY p.taken_at, p.id", select_photo());
        Ok(query_rows(self.conn, &sql, &[("@visit", &visit_id)], photo_row)?)
    }

    pub fn photo_detail(&self, id: i64) -> Result<Option<PhotoDetail>> {
        let sql = format!("{} WHERE p.id = @id", select_photo());
        let Some(photo) = query_rows(self.conn, &sql, &[("@id", &id)], photo_row)?.into_iter().next() else {
            return Ok(None);
        };
        type Extra = (Option<String>, Option<String>, Option<String>, Option<String>, Option<String>, Option<String>);
        let extra: Extra = query_rows(
            self.conn,
            r"SELECT coalesce(w.world_id, p.xmp_world_id), v.instance_id, v.location, v.join_time, v.leave_time,
                     p.xmp_author_id
              FROM photos p
              LEFT JOIN visits v ON v.id = p.visit_id
              LEFT JOIN worlds w ON w.id = v.world_id
              WHERE p.id = @id",
            &[("@id", &id)],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)),
        )?
        .into_iter()
        .next()
        .unwrap_or_default();
        let players = match photo.visit_id {
            Some(visit_id) => query_rows(
                self.conn,
                r"SELECT e.player_id, pl.user_id, e.display_name, pl.is_local
                  FROM encounters e JOIN players pl ON pl.id = e.player_id
                  WHERE e.visit_id = @visit AND e.join_time <= @at AND e.leave_time >= @at
                  ORDER BY pl.is_local DESC, e.display_name, e.player_id",
                &[("@visit", &visit_id), ("@at", &photo.taken_at)],
                |r| {
                    Ok(PhotoPlayer {
                        player_id: r.get(0)?,
                        user_id: r.get(1)?,
                        display_name: r.get(2)?,
                        is_local: r.get(3)?,
                    })
                },
            )?,
            None => Vec::new(),
        };
        Ok(Some(PhotoDetail {
            photo,
            world_id: extra.0,
            instance_id: extra.1,
            location: extra.2,
            visit_join_time: extra.3,
            visit_leave_time: extra.4,
            author_id: extra.5,
            players,
        }))
    }

    pub fn photo_path(&self, id: i64) -> Result<Option<String>> {
        Ok(query_rows(self.conn, "SELECT path FROM photos WHERE id = @id", &[("@id", &id)], |r| r.get(0))?
            .into_iter()
            .next())
    }

    pub fn set_favorite(&self, id: i64, favorite: bool) -> Result<()> {
        self.conn.execute("UPDATE photos SET favorite = ?1 WHERE id = ?2", rusqlite::params![favorite, id])?;
        Ok(())
    }

    /// Number of photos that are not missing.
    pub fn count(&self) -> Result<i64> {
        Ok(self.conn.query_row("SELECT count(*) FROM photos WHERE missing = 0", [], |r| r.get(0))?)
    }

    /// `(id, path, online_only)` of non-missing photos, newest first.
    pub fn photo_ids_newest(&self, limit: usize, offset: usize) -> Result<Vec<(i64, String, bool)>> {
        let (limit, offset) = (limit as i64, offset as i64);
        Ok(query_rows(
            self.conn,
            "SELECT id, path, online_only FROM photos WHERE missing = 0
             ORDER BY taken_at DESC, id DESC LIMIT @limit OFFSET @offset",
            &[("@limit", &limit), ("@offset", &offset)],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?)
    }
}
