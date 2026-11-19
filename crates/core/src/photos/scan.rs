//! Folder scanning: keeps the `photos` table in sync with the files on disk.
//! Rows are never deleted; vanished files are flagged `missing`.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use chrono::{DateTime, Local, NaiveDateTime};
use rusqlite::{params, Connection};
use serde::Serialize;
use walkdir::WalkDir;

use super::meta::read_meta;
use crate::data::db;
use crate::Result;

pub const PHOTO_EXTENSIONS: [&str; 4] = ["png", "jpg", "jpeg", "webp"];

/// True for cloud placeholders (e.g. Dropbox online-only): reading them would download them.
pub fn is_online_only(md: &std::fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const RECALL_ON_DATA_ACCESS: u32 = 0x0040_0000;
        const OFFLINE: u32 = 0x0000_1000;
        md.file_attributes() & (RECALL_ON_DATA_ACCESS | OFFLINE) != 0
    }
    #[cfg(not(windows))]
    {
        let _ = md;
        false
    }
}

#[derive(Debug, Default, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanSummary {
    pub scanned: usize,
    pub added: usize,
    pub updated: usize,
    pub missing: usize,
}

struct Existing {
    id: i64,
    size: i64,
    mtime: String,
    missing: bool,
    online_only: bool,
}

fn has_photo_extension(path: &Path) -> bool {
    path.extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .is_some_and(|e| PHOTO_EXTENSIONS.contains(&e.as_str()))
}

pub fn scan_folders(
    conn: &Connection,
    folders: &[PathBuf],
    progress: Option<&dyn Fn(&str)>,
) -> Result<ScanSummary> {
    let mut existing: HashMap<String, Existing> = HashMap::new();
    {
        let mut stmt = conn.prepare("SELECT id, path, file_size, mtime, missing, online_only FROM photos")?;
        let mut rows = stmt.query([])?;
        while let Some(r) = rows.next()? {
            existing.insert(
                r.get(1)?,
                Existing {
                    id: r.get(0)?,
                    size: r.get(2)?,
                    mtime: r.get(3)?,
                    missing: r.get(4)?,
                    online_only: r.get(5)?,
                },
            );
        }
    }

    let tx = conn.unchecked_transaction()?;
    let mut summary = ScanSummary::default();
    let mut seen: HashSet<i64> = HashSet::new();
    let mut unavailable: Vec<&PathBuf> = Vec::new();
    {
        let mut upsert = tx.prepare(
            r"INSERT INTO photos (path, file_name, file_size, mtime, taken_at, time_source, width, height,
                                  xmp_world_id, xmp_world_name, xmp_author_id, favorite, missing, online_only)
              VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, 0, 0, ?12)
              ON CONFLICT(path) DO UPDATE SET
                file_name = excluded.file_name, file_size = excluded.file_size, mtime = excluded.mtime,
                taken_at = excluded.taken_at, time_source = excluded.time_source,
                width = excluded.width, height = excluded.height,
                xmp_world_id = excluded.xmp_world_id, xmp_world_name = excluded.xmp_world_name,
                xmp_author_id = excluded.xmp_author_id, missing = 0, online_only = excluded.online_only
              RETURNING id",
        )?;

        for folder in folders {
            // An unreadable root (e.g. unplugged drive) must not flag its photos as missing.
            if std::fs::read_dir(folder).is_err() {
                unavailable.push(folder);
                continue;
            }
            for entry in WalkDir::new(folder).follow_links(false).into_iter().filter_map(|e| e.ok()) {
                if !entry.file_type().is_file() || !has_photo_extension(entry.path()) {
                    continue;
                }
                let Ok(md) = entry.metadata() else { continue };
                summary.scanned += 1;
                if summary.scanned % 500 == 0 {
                    if let Some(p) = progress {
                        p(&format!("写真を確認中: {} 件", summary.scanned));
                    }
                }
                let path_str = entry.path().to_string_lossy().into_owned();
                let size = md.len() as i64;
                let mtime_dt: NaiveDateTime = md
                    .modified()
                    .map(|t| DateTime::<Local>::from(t).naive_local())
                    .unwrap_or_else(|_| DateTime::<Local>::from(SystemTime::UNIX_EPOCH).naive_local());
                let mtime = db::ts(mtime_dt);
                let online = is_online_only(&md);
                let prev = existing.get(&path_str);
                if let Some(e) = prev {
                    seen.insert(e.id);
                    if !e.missing && e.size == size && e.mtime == mtime && e.online_only == online {
                        continue;
                    }
                }
                // Online-only files must not be opened (that would download them).
                let meta = read_meta(entry.path(), mtime_dt, !online);
                let id: i64 = upsert.query_row(
                    params![
                        path_str,
                        entry.file_name().to_string_lossy(),
                        size,
                        mtime,
                        db::ts(meta.taken_at),
                        meta.time_source.as_str(),
                        meta.width,
                        meta.height,
                        meta.world_id,
                        meta.world_name,
                        meta.author_id,
                        online
                    ],
                    |r| r.get(0),
                )?;
                seen.insert(id);
                if prev.is_some() {
                    summary.updated += 1;
                } else {
                    summary.added += 1;
                }
            }
        }

        let mut mark = tx.prepare("UPDATE photos SET missing = 1 WHERE id = ?1")?;
        for (path, e) in &existing {
            if e.missing || seen.contains(&e.id) {
                continue;
            }
            if unavailable.iter().any(|f| Path::new(path).starts_with(f)) {
                continue;
            }
            mark.execute([e.id])?;
            summary.missing += 1;
        }
    }
    relink_visits(&tx)?;
    tx.commit()?;
    Ok(summary)
}

/// Re-derives `photos.visit_id` from `taken_at` (visit ids change on every log re-import).
/// Returns the number of rows whose link changed.
pub fn relink_visits(conn: &Connection) -> Result<usize> {
    Ok(conn.execute(
        r"UPDATE photos SET visit_id = n.v
          FROM (SELECT p.id AS id,
                       (SELECT v.id FROM visits v
                         WHERE v.join_time <= p.taken_at AND v.leave_time >= p.taken_at
                         ORDER BY v.join_time DESC LIMIT 1) AS v
                FROM photos p) AS n
          WHERE n.id = photos.id AND photos.visit_id IS NOT n.v",
        [],
    )?)
}
