//! Planning and applying "write log-derived metadata into photos" (and undoing it).
//!
//! Nothing here touches a file unless [`MetadataService::apply_write`] / [`apply_remove`] is
//! called, and both re-check every condition right before acting.
//!
//! [`apply_remove`]: MetadataService::apply_remove

use std::collections::HashSet;
use std::fs::Metadata;
use std::path::Path;

use chrono::{Duration, NaiveDate, NaiveDateTime};
use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use super::db;
use super::query::query_rows;
use crate::photos::cloud::hydrate;
use crate::photos::{
    is_online_only, refresh_file, remove_our_xmp_from_file, write_xmp_to_file, xmp_presence, XmpFields, XmpPresence,
};
use crate::{Error, Result};

/// Which photos an operation is about.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum MetaTarget {
    Ids { ids: Vec<i64> },
    Day { day: NaiveDate },
    Album { album_id: i64 },
    Range { from: NaiveDate, to: NaiveDate },
    /// Every photo in the library that is not flagged missing.
    All,
}

/// Why a photo is not eligible. Declaration order is the order used in plans.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SkipReason {
    /// Write: the file already has XMP. Remove: nothing of ours to remove.
    HasMetadata,
    NoLog,
    EstimatedTime,
    OnlineOnly,
    Missing,
    UnsupportedFormat,
    ReadOnly,
    Unreadable,
}

impl SkipReason {
    pub const ALL: [SkipReason; 8] = [
        SkipReason::HasMetadata,
        SkipReason::NoLog,
        SkipReason::EstimatedTime,
        SkipReason::OnlineOnly,
        SkipReason::Missing,
        SkipReason::UnsupportedFormat,
        SkipReason::ReadOnly,
        SkipReason::Unreadable,
    ];
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkipCount {
    pub reason: SkipReason,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MetaExample {
    pub file_name: String,
    pub taken_at: String,
    pub world_name: String,
    pub world_id: String,
    pub author: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MetaPlan {
    pub total: usize,
    pub eligible: usize,
    pub eligible_ids: Vec<i64>,
    /// Only reasons that occurred, in declaration order.
    pub skipped: Vec<SkipCount>,
    /// First eligible photo of a write plan (`None` for remove plans).
    pub example: Option<MetaExample>,
    /// Write plans: online-only photos in the target. Each is either in `download_ids` or
    /// counted in `skipped` by a reason known without reading it.
    pub online: usize,
    /// Write plans: online-only photos that pass every check possible without the content. They
    /// are only written when the run is allowed to download them (they may still turn out to
    /// already have metadata).
    pub download: usize,
    pub download_ids: Vec<i64>,
    pub download_bytes: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum MetaOutcome {
    Done,
    Skipped(SkipReason),
    Failed(String),
}

/// Result of [`MetadataService::apply_write_downloading`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadOutcome {
    pub outcome: MetaOutcome,
    /// The file was an online-only placeholder and was downloaded (it stays local).
    pub downloaded: bool,
}

type Verdict<T> = std::result::Result<T, SkipReason>;

struct PhotoRef {
    path: String,
    file_name: String,
    taken_at: String,
    time_source: String,
    online_only: bool,
    file_size: i64,
    /// The library already read XMP from this file at some point.
    xmp_known: bool,
}

pub struct MetadataService<'a> {
    conn: &'a Connection,
}

/// `[start, end)` of a logical day as stored timestamps (same rule as the timeline).
fn day_window(day: NaiveDate, day_start_hour: u32) -> (String, String) {
    let start = day.and_hms_opt(day_start_hour.min(23), 0, 0).expect("hour is clamped to 0..=23");
    (db::ts(start), db::ts(start + Duration::days(1)))
}

fn has_png_or_jpeg_extension(path: &Path) -> bool {
    path.extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .is_some_and(|e| matches!(e.as_str(), "png" | "jpg" | "jpeg"))
}

impl<'a> MetadataService<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    /// Photo ids of a target. Day / Range skip photos flagged missing (like the timeline);
    /// Album keeps them (they are classified `Missing` later); Ids keeps the given order and
    /// drops unknown and repeated ids.
    pub fn resolve(&self, target: &MetaTarget, day_start_hour: u32) -> Result<Vec<i64>> {
        let by_time = "SELECT id FROM photos WHERE missing = 0 AND taken_at >= @s AND taken_at < @e ORDER BY taken_at, id";
        match target {
            MetaTarget::Ids { ids } => {
                let mut seen = HashSet::new();
                let mut out = Vec::new();
                for &id in ids {
                    if !seen.insert(id) {
                        continue;
                    }
                    let known: Option<i64> =
                        self.conn.query_row("SELECT id FROM photos WHERE id = ?1", [id], |r| r.get(0)).optional()?;
                    out.extend(known);
                }
                Ok(out)
            }
            MetaTarget::Day { day } => {
                let (s, e) = day_window(*day, day_start_hour);
                Ok(query_rows(self.conn, by_time, &[("@s", &s), ("@e", &e)], |r| r.get(0))?)
            }
            MetaTarget::Range { from, to } => {
                if from > to {
                    return Ok(Vec::new());
                }
                let (s, e) = (db::ts(from.and_time(Default::default())), db::ts((*to + Duration::days(1)).and_time(Default::default())));
                Ok(query_rows(self.conn, by_time, &[("@s", &s), ("@e", &e)], |r| r.get(0))?)
            }
            MetaTarget::Album { album_id } => Ok(query_rows(
                self.conn,
                "SELECT photo_id FROM album_photos WHERE album_id = @a ORDER BY position, photo_id",
                &[("@a", album_id)],
                |r| r.get(0),
            )?),
            MetaTarget::All => {
                Ok(query_rows(self.conn, "SELECT id FROM photos WHERE missing = 0 ORDER BY taken_at, id", &[], |r| r.get(0))?)
            }
        }
    }

    fn photo_ref(&self, id: i64) -> Result<Option<(PhotoRef, bool)>> {
        Ok(self
            .conn
            .query_row(
                r"SELECT path, file_name, taken_at, time_source, online_only, missing, file_size,
                         time_source = 'xmp' OR xmp_world_id IS NOT NULL OR xmp_author_id IS NOT NULL
                  FROM photos WHERE id = ?1",
                [id],
                |r| {
                    Ok((
                        PhotoRef {
                            path: r.get(0)?,
                            file_name: r.get(1)?,
                            taken_at: r.get(2)?,
                            time_source: r.get(3)?,
                            online_only: r.get(4)?,
                            file_size: r.get(6)?,
                            xmp_known: r.get(7)?,
                        },
                        r.get::<_, bool>(5)?,
                    ))
                },
            )
            .optional()?)
    }

    /// Row present and not missing, file on disk. Only file attributes are looked at.
    fn locate(&self, id: i64) -> Result<Verdict<(PhotoRef, Metadata)>> {
        let Some((row, missing)) = self.photo_ref(id)? else {
            return Ok(Err(SkipReason::Missing));
        };
        if missing {
            return Ok(Err(SkipReason::Missing));
        }
        match std::fs::metadata(&row.path) {
            Ok(md) => Ok(Ok((row, md))),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Err(SkipReason::Missing)),
            Err(_) => Ok(Err(SkipReason::Unreadable)),
        }
    }

    /// [`Self::locate`], and the file must not be online-only. Never reads the content.
    fn probe(&self, id: i64) -> Result<Verdict<(PhotoRef, Metadata)>> {
        let (row, md) = match self.locate(id)? {
            Ok(x) => x,
            Err(r) => return Ok(Err(r)),
        };
        if row.online_only || is_online_only(&md) {
            return Ok(Err(SkipReason::OnlineOnly));
        }
        Ok(Ok((row, md)))
    }

    /// Decides whether metadata may be written and, if so, exactly what.
    pub fn classify_write(
        &self,
        photo_id: i64,
        utc_offset_minutes: i32,
        now: NaiveDateTime,
    ) -> Result<Verdict<(String, XmpFields)>> {
        let (row, md) = match self.probe(photo_id)? {
            Ok(x) => x,
            Err(r) => return Ok(Err(r)),
        };
        self.write_fields(row, &md, true, utc_offset_minutes, now)
    }

    /// Checks of [`Self::classify_write`] after the location checks. With `read_content` false
    /// (online-only files) the file's XMP is judged only by what the library already knows.
    fn write_fields(
        &self,
        row: PhotoRef,
        md: &Metadata,
        read_content: bool,
        utc_offset_minutes: i32,
        now: NaiveDateTime,
    ) -> Result<Verdict<(String, XmpFields)>> {
        let path = Path::new(&row.path);
        if !has_png_or_jpeg_extension(path) {
            return Ok(Err(SkipReason::UnsupportedFormat));
        }
        if row.time_source == "mtime" {
            return Ok(Err(SkipReason::EstimatedTime));
        }
        if md.permissions().readonly() {
            return Ok(Err(SkipReason::ReadOnly));
        }
        if read_content {
            match xmp_presence(path) {
                Err(_) => return Ok(Err(SkipReason::Unreadable)),
                Ok(XmpPresence::None) => {}
                Ok(_) => return Ok(Err(SkipReason::HasMetadata)),
            }
        } else if row.xmp_known {
            return Ok(Err(SkipReason::HasMetadata));
        }
        let taken_at = NaiveDateTime::parse_from_str(&row.taken_at, "%Y-%m-%d %H:%M:%S")
            .map_err(|e| Error::Invalid(format!("bad taken_at '{}': {e}", row.taken_at)))?;

        let visit: Option<(i64, String, String)> = self
            .conn
            .query_row(
                r"SELECT v.id, v.world_name, w.world_id
                  FROM visits v JOIN worlds w ON w.id = v.world_id
                  WHERE v.join_time <= ?1 AND v.leave_time >= ?1 AND substr(w.world_id, 1, 5) = 'wrld_'
                  ORDER BY v.join_time DESC, v.id DESC LIMIT 1",
                [&row.taken_at],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?;
        let Some((visit_id, world_name, world_id)) = visit else {
            return Ok(Err(SkipReason::NoLog));
        };

        let (author, author_id) = self.local_author(visit_id)?;
        let lower = row.file_name.to_ascii_lowercase();
        let creator_tool = (lower.starts_with("vrchat_") || lower.starts_with("vrc_")).then(|| "VRChat".to_string());
        Ok(Ok((
            row.path,
            XmpFields {
                creator_tool,
                author,
                author_id,
                create_date: taken_at,
                utc_offset_minutes,
                world_id,
                world_name,
                written_at: now,
            },
        )))
    }

    /// The local player in that visit; else the only local player known at all; else nobody.
    fn local_author(&self, visit_id: i64) -> Result<(Option<String>, Option<String>)> {
        let in_visit: Option<(String, String)> = self
            .conn
            .query_row(
                r"SELECT e.display_name, p.user_id FROM encounters e JOIN players p ON p.id = e.player_id
                  WHERE e.visit_id = ?1 AND p.is_local = 1 ORDER BY e.join_time, e.id LIMIT 1",
                [visit_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        if let Some((name, id)) = in_visit {
            return Ok((Some(name), Some(id)));
        }
        let locals: Vec<(String, String)> = query_rows(
            self.conn,
            "SELECT display_name, user_id FROM players WHERE is_local = 1 LIMIT 2",
            &[],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        Ok(match <[_; 1]>::try_from(locals) {
            Ok([(name, id)]) => (Some(name), Some(id)),
            Err(_) => (None, None),
        })
    }

    /// Eligible iff the file is local, present, writable and carries XMP written by this tool.
    /// `HasMetadata` here means "nothing of ours to remove".
    pub fn classify_remove(&self, photo_id: i64) -> Result<Verdict<String>> {
        let (row, md) = match self.probe(photo_id)? {
            Ok(x) => x,
            Err(r) => return Ok(Err(r)),
        };
        if md.permissions().readonly() {
            return Ok(Err(SkipReason::ReadOnly));
        }
        match xmp_presence(Path::new(&row.path)) {
            Err(_) => Ok(Err(SkipReason::Unreadable)),
            Ok(XmpPresence::Ours) => Ok(Ok(row.path)),
            Ok(_) => Ok(Err(SkipReason::HasMetadata)),
        }
    }

    /// Online-only photos are never read here: they go to `download_ids` or are skipped by a
    /// reason known from the library alone.
    pub fn plan_write(&self, ids: &[i64], utc_offset_minutes: i32, now: NaiveDateTime) -> Result<MetaPlan> {
        let mut plan = PlanBuilder::new(ids.len());
        for &id in ids {
            let (row, md) = match self.locate(id)? {
                Ok(x) => x,
                Err(r) => {
                    plan.skip(r);
                    continue;
                }
            };
            let online = row.online_only || is_online_only(&md);
            let size = row.file_size;
            let verdict = self.write_fields(row, &md, !online, utc_offset_minutes, now)?;
            if online {
                plan.online += 1;
                match verdict {
                    Ok(_) => plan.download(id, size),
                    Err(r) => plan.skip(r),
                }
                continue;
            }
            match verdict {
                Ok((path, f)) => {
                    let example = plan.example.is_none().then(|| MetaExample {
                        file_name: Path::new(&path)
                            .file_name()
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_default(),
                        taken_at: db::ts(f.create_date),
                        world_name: f.world_name,
                        world_id: f.world_id,
                        author: f.author,
                    });
                    plan.eligible(id, example);
                }
                Err(r) => plan.skip(r),
            }
        }
        Ok(plan.finish())
    }

    pub fn plan_remove(&self, ids: &[i64]) -> Result<MetaPlan> {
        let mut plan = PlanBuilder::new(ids.len());
        for &id in ids {
            match self.classify_remove(id)? {
                Ok(_) => plan.eligible(id, None),
                Err(r) => plan.skip(r),
            }
        }
        Ok(plan.finish())
    }

    /// Writes the metadata (after re-classifying) and refreshes the photo's row. File errors
    /// become [`MetaOutcome::Failed`]; only database errors are returned as `Err`.
    pub fn apply_write(&self, photo_id: i64, utc_offset_minutes: i32, now: NaiveDateTime) -> Result<MetaOutcome> {
        let (path, fields) = match self.classify_write(photo_id, utc_offset_minutes, now)? {
            Ok(x) => x,
            Err(r) => return Ok(MetaOutcome::Skipped(r)),
        };
        self.finish(&path, write_xmp_to_file(Path::new(&path), &fields).map(|()| true))
    }

    /// Like [`Self::apply_write`], but an online-only file is downloaded first, only if it passes
    /// every check possible without its content. The downloaded file stays local.
    pub fn apply_write_downloading(
        &self,
        photo_id: i64,
        utc_offset_minutes: i32,
        now: NaiveDateTime,
    ) -> Result<DownloadOutcome> {
        let plain = |outcome| DownloadOutcome { outcome, downloaded: false };
        let (row, md) = match self.locate(photo_id)? {
            Ok(x) => x,
            Err(r) => return Ok(plain(MetaOutcome::Skipped(r))),
        };
        let placeholder = is_online_only(&md);
        if !row.online_only && !placeholder {
            return self.apply_write(photo_id, utc_offset_minutes, now).map(plain);
        }
        let path = row.path.clone();
        if let Err(r) = self.write_fields(row, &md, false, utc_offset_minutes, now)? {
            return Ok(plain(MetaOutcome::Skipped(r)));
        }
        if placeholder {
            if let Err(e) = hydrate(Path::new(&path)) {
                return Ok(plain(MetaOutcome::Failed(format!("could not download the file: {e}"))));
            }
        }
        // Record the now-local file (its XMP, if any) before deciding; `apply_write` re-checks it.
        let outcome = match refresh_file(self.conn, Path::new(&path)) {
            Ok(_) => self.apply_write(photo_id, utc_offset_minutes, now)?,
            Err(e) => MetaOutcome::Failed(format!("the library could not be refreshed: {e}")),
        };
        Ok(DownloadOutcome { outcome, downloaded: placeholder })
    }

    pub fn apply_remove(&self, photo_id: i64) -> Result<MetaOutcome> {
        let path = match self.classify_remove(photo_id)? {
            Ok(p) => p,
            Err(r) => return Ok(MetaOutcome::Skipped(r)),
        };
        self.finish(&path, remove_our_xmp_from_file(Path::new(&path)))
    }

    fn finish(&self, path: &str, op: Result<bool>) -> Result<MetaOutcome> {
        // The row is refreshed even after a failure: the file may have changed anyway.
        let refreshed = refresh_file(self.conn, Path::new(path));
        Ok(match (op, refreshed) {
            (Err(e), _) => MetaOutcome::Failed(e.to_string()),
            (Ok(false), _) => MetaOutcome::Skipped(SkipReason::HasMetadata),
            (Ok(true), Err(e)) => MetaOutcome::Failed(format!("the file was updated but the library could not be refreshed: {e}")),
            (Ok(true), Ok(_)) => MetaOutcome::Done,
        })
    }
}

struct PlanBuilder {
    total: usize,
    eligible_ids: Vec<i64>,
    counts: [usize; SkipReason::ALL.len()],
    example: Option<MetaExample>,
    online: usize,
    download_ids: Vec<i64>,
    download_bytes: i64,
}

impl PlanBuilder {
    fn new(total: usize) -> Self {
        Self {
            total,
            eligible_ids: Vec::new(),
            counts: [0; SkipReason::ALL.len()],
            example: None,
            online: 0,
            download_ids: Vec::new(),
            download_bytes: 0,
        }
    }

    fn download(&mut self, id: i64, size: i64) {
        self.download_ids.push(id);
        self.download_bytes += size.max(0);
    }

    fn eligible(&mut self, id: i64, example: Option<MetaExample>) {
        self.eligible_ids.push(id);
        if self.example.is_none() {
            self.example = example;
        }
    }

    fn skip(&mut self, reason: SkipReason) {
        self.counts[reason as usize] += 1;
    }

    fn finish(self) -> MetaPlan {
        MetaPlan {
            total: self.total,
            eligible: self.eligible_ids.len(),
            eligible_ids: self.eligible_ids,
            skipped: SkipReason::ALL
                .iter()
                .filter(|&&r| self.counts[r as usize] > 0)
                .map(|&r| SkipCount { reason: r, count: self.counts[r as usize] })
                .collect(),
            example: self.example,
            online: self.online,
            download: self.download_ids.len(),
            download_ids: self.download_ids,
            download_bytes: self.download_bytes,
        }
    }
}
