//! Tags, player memos and profiles, and the first-met / reunion badges.
use super::*;

impl LibraryService<'_> {
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
        let name = non_empty(NameKind::Tag, name)?;
        let color = valid_color(color);
        self.conn
            .execute("INSERT OR IGNORE INTO tags(name, color) VALUES (?1, ?2)", params![name, color])?;
        Ok(self.conn.query_row("SELECT id, name, color FROM tags WHERE name = ?1", [&name], tag_row)?)
    }

    pub fn update_tag(&self, id: i64, name: &str, color: &str) -> Result<()> {
        let name = non_empty(NameKind::Tag, name)?;
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
    pub(super) fn visit_people(&self, visit_id: i64, visit_join: &str, reunion_days: i32) -> Result<Vec<TimelinePlayer>> {
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
    /// `days_since_last` is set only when `reunion_days > 0` and the gap reaches it;
    /// `show_first_met = false` drops the first-met badge (players left with no badge are omitted).
    pub fn visit_player_badges(
        &self,
        visit_id: i64,
        reunion_days: i32,
        show_first_met: bool,
    ) -> Result<Vec<PlayerBadge>> {
        let join: Option<String> = self
            .conn
            .query_row("SELECT join_time FROM visits WHERE id = ?1", [visit_id], |r| r.get(0))
            .optional()?;
        let Some(join) = join else { return Ok(Vec::new()) };
        Ok(self
            .visit_people(visit_id, &join, reunion_days)?
            .into_iter()
            .map(|p| PlayerBadge {
                player_id: p.player_id,
                first_met: p.first_met && show_first_met,
                days_since_last: p.days_since_last,
            })
            .filter(|b| b.first_met || b.days_since_last.is_some())
            .collect())
    }
}

fn tag_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Tag> {
    Ok(Tag { id: r.get(0)?, name: r.get(1)?, color: r.get(2)? })
}
