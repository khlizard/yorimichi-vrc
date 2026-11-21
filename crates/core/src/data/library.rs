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

use super::db::{self, TS_FORMAT};
use super::dtos::{format_duration, PlayerHit};
use super::library_dtos::*;
use super::photo_query::{photo_row, select_photo, PhotoRow, PhotoService};
use super::query::{player_hit, query_rows, PLAYER_SEARCH_SELECT};
use crate::{LibraryError, NameKind, Result};

const DEFAULT_TAG_COLOR: &str = "#6b7280";
/// A single visit/encounter never counts for more than this (broken log rows).
const MAX_SECONDS: i64 = 24 * 3600;
const IN_CHUNK: usize = 500;

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

fn non_empty(kind: NameKind, s: &str) -> Result<String> {
    let t = s.trim();
    if t.is_empty() {
        return Err(LibraryError::EmptyName(kind).into());
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
}

mod albums;
mod players;
mod timeline;
mod worlds;
