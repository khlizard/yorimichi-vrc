//! Differential log import, pruning and backfill. Port of `ImportService.cs`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use chrono::{Duration, Local, NaiveDateTime};
use rusqlite::{named_params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use super::db;
use crate::parsing::{self, ParseOptions, ParseResult, ParsedUrlEvent};
use crate::Result;

/// Optional progress callback receiving the same (Japanese) messages as the C# app.
pub type Progress<'a> = Option<&'a dyn Fn(&str)>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportSummary {
    pub scanned: usize,
    pub imported: usize,
    pub skipped: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PruneSummary {
    pub expired_deleted: usize,
    pub noise_deleted: usize,
}

impl PruneSummary {
    pub fn total(&self) -> usize {
        self.expired_deleted + self.noise_deleted
    }
}

/// Imports VRChat log files into the database on the given connection.
/// Use a dedicated connection for imports (not the one used for queries).
pub struct ImportService<'a> {
    conn: &'a Connection,
    parse_options: ParseOptions,
}

impl<'a> ImportService<'a> {
    /// Uses the default noise list ([`ParseOptions::default`]).
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn, parse_options: ParseOptions::default() }
    }

    pub fn with_options(conn: &'a Connection, parse_options: ParseOptions) -> Self {
        Self { conn, parse_options }
    }

    /// Deletes raw_events older than `retention_days` (0 = keep forever) and rows matching
    /// the noise patterns (so an existing database catches up with the parser's exclusions).
    /// Structured tables are never touched. Caller decides whether to VACUUM.
    /// `now` defaults to the local current time.
    pub fn prune<S: AsRef<str>>(
        &self,
        retention_days: i32,
        noise_patterns: &[S],
        now: Option<NaiveDateTime>,
    ) -> Result<PruneSummary> {
        let tx = self.conn.unchecked_transaction()?;
        let mut expired = 0;
        if retention_days > 0 {
            let now = now.unwrap_or_else(|| Local::now().naive_local());
            let cutoff = db::ts(now - Duration::days(retention_days as i64));
            expired = tx.execute(
                "DELETE FROM raw_events WHERE event_time < @cutoff",
                named_params! {"@cutoff": cutoff},
            )?;
        }
        let mut noise = 0;
        for p in noise_patterns {
            let p = p.as_ref();
            if p.is_empty() {
                continue;
            }
            let contains = p.starts_with('*');
            let body = if contains { &p[1..] } else { p };
            if body.is_empty() {
                continue;
            }
            let pattern = format!("{}{}%", if contains { "%" } else { "" }, db::escape_like(body));
            noise += tx.execute(
                r"DELETE FROM raw_events WHERE message LIKE @pattern ESCAPE '\'",
                named_params! {"@pattern": pattern},
            )?;
        }
        tx.commit()?;
        Ok(PruneSummary { expired_deleted: expired, noise_deleted: noise })
    }

    /// Scans the given folders for `output_log_*.txt` and imports new or grown files.
    /// When the same file name exists in several folders the largest copy is used.
    pub fn import_folders<P: AsRef<Path>>(
        &self,
        folders: &[P],
        progress: Progress<'_>,
    ) -> Result<ImportSummary> {
        let report = |m: &str| {
            if let Some(f) = progress {
                f(m);
            }
        };
        let (mut scanned, mut imported, mut skipped) = (0, 0, 0);
        let mut candidates: Vec<PathBuf> = Vec::new();
        for folder in folders {
            let folder = folder.as_ref();
            if !folder.is_dir() {
                report(&format!("フォルダなし(スキップ): {}", folder.display()));
                continue;
            }
            for entry in std::fs::read_dir(folder)?.flatten() {
                let path = entry.path();
                let Some(name) = path.file_name().and_then(|n| n.to_str()) else { continue };
                let lower = name.to_lowercase();
                if lower.starts_with("output_log_") && lower.ends_with(".txt") && path.is_file() {
                    candidates.push(path);
                }
            }
        }
        // Same log may exist in several folders (e.g. a copy in the repo and the live
        // one VRChat is still writing) - import the largest copy of each name.
        let mut by_name: HashMap<String, (PathBuf, u64)> = HashMap::new();
        let mut order: Vec<String> = Vec::new();
        for path in candidates {
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            let len = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
            match by_name.get_mut(&name) {
                Some(best) => {
                    if len > best.1 {
                        *best = (path, len);
                    }
                }
                None => {
                    order.push(name.clone());
                    by_name.insert(name, (path, len));
                }
            }
        }
        order.sort();
        for name in order {
            let path = &by_name[&name].0;
            scanned += 1;
            if self.import_file(path, progress)? {
                imported += 1;
            } else {
                skipped += 1;
            }
        }
        report(&format!("完了: {imported} 取り込み / {skipped} 変更なし"));
        Ok(ImportSummary { scanned, imported, skipped })
    }

    /// Marks every known file as not yet imported so the next [`import_folders`](Self::import_folders)
    /// reparses all of them (e.g. after the parser learned to extract new event types).
    pub fn mark_all_for_reimport(&self) -> Result<usize> {
        Ok(self.conn.execute("UPDATE log_files SET last_size = 0", [])?)
    }

    /// Recovers avatar switches from raw_events lines still in the database (the log files
    /// themselves may already be gone). Sizes cannot be recovered: the download lines are
    /// on the noise list. Idempotent; one row per (visit, player, avatar).
    pub fn backfill_avatar_events_from_raw_events(&self) -> Result<usize> {
        const PREFIX: &str = "[Behaviour] Switching ";
        let n = self.conn.execute(
            r"
            INSERT INTO avatar_events (file_id, visit_id, player_id, display_name, avatar_name, event_time)
            SELECT r.file_id, r.visit_id,
                   (SELECT e.player_id FROM encounters e WHERE e.visit_id = r.visit_id AND e.display_name = s.name LIMIT 1),
                   s.name, s.avatar, r.event_time
            FROM raw_events r
            JOIN (SELECT id,
                         substr(message, @plen + 1, instr(message, ' to avatar ') - @plen - 1) AS name,
                         substr(message, instr(message, ' to avatar ') + 11) AS avatar
                  FROM raw_events
                  WHERE message LIKE @like ESCAPE '\' AND instr(message, ' to avatar ') > @plen) s ON s.id = r.id
            WHERE NOT EXISTS (
                SELECT 1 FROM avatar_events a
                WHERE a.visit_id IS r.visit_id AND a.display_name = s.name AND a.avatar_name = s.avatar
                  AND (r.visit_id IS NOT NULL OR a.file_id = r.file_id))
            GROUP BY r.visit_id, s.name, s.avatar
            ",
            named_params! {
                "@plen": PREFIX.chars().count() as i64,
                "@like": format!("{}% to avatar %", db::escape_like(PREFIX)),
            },
        )?;
        Ok(n)
    }

    /// Imports one file. Returns `true` if it was (re)imported, `false` if unchanged
    /// (same byte length as at the last import).
    pub fn import_file(&self, path: &Path, progress: Progress<'_>) -> Result<bool> {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        // Read the file (shared mode, so the log VRChat is still writing works).
        let mut file = std::fs::File::open(path)?;
        let size = file.metadata()?.len() as i64;

        let existing: Option<(i64, i64)> = self
            .conn
            .query_row(
                "SELECT id, last_size FROM log_files WHERE file_name = @name",
                named_params! {"@name": name},
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        if let Some((_, last_size)) = existing {
            if last_size == size {
                return Ok(false);
            }
        }

        if let Some(f) = progress {
            f(&format!("取り込み中: {name}"));
        }
        let result: ParseResult = {
            use std::io::Read;
            let mut bytes = Vec::new();
            file.read_to_end(&mut bytes)?;
            drop(file);
            parsing::parse_bytes(&bytes, &self.parse_options)
        };

        let tx = self.conn.unchecked_transaction()?;
        let c: &Connection = &tx;
        if let Some((id, _)) = existing {
            // File grew (or was replaced): wipe its rows via CASCADE and reparse from scratch.
            c.execute("DELETE FROM log_files WHERE id = @Id", named_params! {"@Id": id})?;
        }
        let now = db::ts(Local::now().naive_local());
        let file_id: i64 = c.query_row(
            "INSERT INTO log_files (file_name, last_size, imported_at) VALUES (@name, @size, @now) RETURNING id",
            named_params! {"@name": name, "@size": size, "@now": now},
            |r| r.get(0),
        )?;

        let mut visit_ids: Vec<i64> = Vec::with_capacity(result.visits.len());
        // display name -> player id, per visit, for linking avatar events (which only carry names).
        let mut players_by_name: Vec<HashMap<String, i64>> = Vec::with_capacity(result.visits.len());
        for v in &result.visits {
            let mut names: HashMap<String, i64> = HashMap::new();
            let world_row_id: Option<i64> = match &v.world_id {
                None => None,
                Some(wid) => Some(c.prepare_cached(
                    r"
                    INSERT INTO worlds (world_id, name) VALUES (@WorldId, @WorldName)
                    ON CONFLICT(world_id) DO UPDATE SET name = CASE WHEN excluded.name <> '' THEN excluded.name ELSE name END
                    RETURNING id
                    ",
                )?.query_row(
                    named_params! {"@WorldId": wid, "@WorldName": v.world_name},
                    |r| r.get(0),
                )?),
            };

            let visit_id: i64 = c.prepare_cached(
                r"
                INSERT INTO visits (file_id, world_id, world_name, instance_id, location, join_time, leave_time, ended_cleanly)
                VALUES (@fileId, @worldRowId, @WorldName, @InstanceId, @Location, @join, @leave, @clean)
                RETURNING id
                ",
            )?.query_row(
                named_params! {
                    "@fileId": file_id,
                    "@worldRowId": world_row_id,
                    "@WorldName": v.world_name,
                    "@InstanceId": v.instance_id,
                    "@Location": v.location,
                    "@join": db::ts(v.join_time),
                    "@leave": db::ts(v.leave_time),
                    "@clean": v.ended_cleanly,
                },
                |r| r.get(0),
            )?;
            visit_ids.push(visit_id);

            for enc in &v.encounters {
                let player_id = upsert_player(
                    c,
                    &enc.user_id,
                    &enc.display_name,
                    result.local_user_id.as_deref() == Some(enc.user_id.as_str()),
                    &db::ts(enc.join_time),
                    &db::ts(enc.leave_time),
                )?;
                names.insert(enc.display_name.clone(), player_id);
                c.prepare_cached(
                    r"
                    INSERT INTO encounters (visit_id, player_id, display_name, join_time, leave_time, ended_cleanly)
                    VALUES (@visitId, @playerId, @DisplayName, @join, @leave, @clean)
                    ",
                )?.execute(named_params! {
                    "@visitId": visit_id,
                    "@playerId": player_id,
                    "@DisplayName": enc.display_name,
                    "@join": db::ts(enc.join_time),
                    "@leave": db::ts(enc.leave_time),
                    "@clean": enc.ended_cleanly,
                })?;
            }
            players_by_name.push(names);

            for u in &v.url_events {
                insert_url(c, Some(visit_id), file_id, u)?;
            }
        }

        if let (Some(uid), Some(uname), Some(last)) =
            (&result.local_user_id, &result.local_user_name, result.last_timestamp)
        {
            upsert_player(c, uid, uname, true, &db::ts(last), &db::ts(last))?;
        }

        for u in &result.orphan_url_events {
            insert_url(c, None, file_id, u)?;
        }

        // Notifications are not tied to a file: the same notification id is re-logged on every
        // login, so it may appear in several files and must survive any one of them being reimported.
        for n in &result.notifications {
            c.prepare_cached(
                r"
                INSERT INTO notifications (notification_id, kind, sender_user_id, sender_name, message, details, created_at_utc, event_time)
                VALUES (@NotificationId, @Kind, @SenderUserId, @SenderName, @Message, @Details, @CreatedAtUtc, @time)
                ON CONFLICT(notification_id) DO UPDATE SET event_time = min(event_time, excluded.event_time)
                ",
            )?.execute(named_params! {
                "@NotificationId": n.notification_id,
                "@Kind": n.kind,
                "@SenderUserId": n.sender_user_id,
                "@SenderName": n.sender_name,
                "@Message": n.message,
                "@Details": n.details,
                "@CreatedAtUtc": n.created_at_utc,
                "@time": db::ts(n.time),
            })?;
        }

        for (notification_id, t) in &result.answered_notifications {
            c.prepare_cached(
                "UPDATE notifications SET accepted_at = coalesce(accepted_at, @time) WHERE notification_id = @notificationId",
            )?.execute(named_params! {
                "@notificationId": notification_id,
                "@time": db::ts(*t),
            })?;
        }

        for me in &result.moderation_events {
            c.prepare_cached(
                r"
                INSERT INTO moderation_events (file_id, visit_id, kind, target_name, event_time)
                VALUES (@fileId, @visitId, @Kind, @TargetName, @time)
                ",
            )?.execute(named_params! {
                "@fileId": file_id,
                "@visitId": me.visit_index.map(|i| visit_ids[i]),
                "@Kind": me.kind,
                "@TargetName": me.target_name,
                "@time": db::ts(me.time),
            })?;
        }

        for av in &result.avatar_events {
            let mut player_id: Option<i64> = None;
            let linked = av
                .visit_index
                .and_then(|i| players_by_name[i].get(&av.player_name).copied());
            if let Some(pid) = linked {
                player_id = Some(pid);
            } else {
                // Outside a visit (or name not among encounters): fall back to a unique current display name.
                let mut stmt =
                    c.prepare_cached("SELECT id FROM players WHERE display_name = @name LIMIT 2")?;
                let ids: Vec<i64> = stmt
                    .query_map(named_params! {"@name": av.player_name}, |r| r.get(0))?
                    .collect::<std::result::Result<_, _>>()?;
                if ids.len() == 1 {
                    player_id = Some(ids[0]);
                }
            }
            c.prepare_cached(
                r"
                INSERT INTO avatar_events (file_id, visit_id, player_id, display_name, avatar_name, size_mb, load_seconds, event_time)
                VALUES (@fileId, @visitId, @playerId, @PlayerName, @AvatarName, @SizeMb, @LoadSeconds, @time)
                ",
            )?.execute(named_params! {
                "@fileId": file_id,
                "@visitId": av.visit_index.map(|i| visit_ids[i]),
                "@playerId": player_id,
                "@PlayerName": av.player_name,
                "@AvatarName": av.avatar_name,
                "@SizeMb": av.size_mb,
                "@LoadSeconds": av.load_seconds,
                "@time": db::ts(av.time),
            })?;
        }

        for raw in &result.raw_events {
            c.prepare_cached(
                r"
                INSERT INTO raw_events (file_id, visit_id, event_time, level, message)
                VALUES (@fileId, @visitId, @time, @Level, @Message)
                ",
            )?.execute(named_params! {
                "@fileId": file_id,
                "@visitId": raw.visit_index.map(|i| visit_ids[i]),
                "@time": db::ts(raw.time),
                "@Level": raw.level,
                "@Message": raw.message,
            })?;
        }

        tx.commit()?;
        Ok(true)
    }
}

fn upsert_player(
    c: &Connection,
    user_id: &str,
    display_name: &str,
    is_local: bool,
    first_seen: &str,
    last_seen: &str,
) -> Result<i64> {
    let player_id: i64 = c.prepare_cached(
        r"
        INSERT INTO players (user_id, display_name, is_local) VALUES (@userId, @displayName, @isLocal)
        ON CONFLICT(user_id) DO UPDATE SET
          display_name = CASE WHEN @lastSeen >= (SELECT max(last_seen) FROM player_names WHERE player_id = players.id)
                              THEN excluded.display_name ELSE display_name END,
          is_local = max(is_local, excluded.is_local)
        RETURNING id
        ",
    )?.query_row(
        named_params! {
            "@userId": user_id,
            "@displayName": display_name,
            "@isLocal": is_local,
            "@lastSeen": last_seen,
        },
        |r| r.get(0),
    )?;
    c.prepare_cached(
        r"
        INSERT INTO player_names (player_id, display_name, first_seen, last_seen)
        VALUES (@playerId, @displayName, @firstSeen, @lastSeen)
        ON CONFLICT(player_id, display_name) DO UPDATE SET
          first_seen = min(first_seen, excluded.first_seen),
          last_seen = max(last_seen, excluded.last_seen)
        ",
    )?.execute(named_params! {
        "@playerId": player_id,
        "@displayName": display_name,
        "@firstSeen": first_seen,
        "@lastSeen": last_seen,
    })?;
    Ok(player_id)
}

fn insert_url(c: &Connection, visit_id: Option<i64>, file_id: i64, u: &ParsedUrlEvent) -> Result<()> {
    c.prepare_cached(
        "INSERT INTO url_events (visit_id, file_id, kind, url, event_time) VALUES (@visitId, @fileId, @Kind, @Url, @time)",
    )?.execute(named_params! {
        "@visitId": visit_id,
        "@fileId": file_id,
        "@Kind": u.kind,
        "@Url": u.url,
        "@time": db::ts(u.time),
    })?;
    Ok(())
}
