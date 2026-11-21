//! World list and world notes.
use super::*;

impl LibraryService<'_> {
    // ---- worlds ------------------------------------------------------------------------

    fn world_select() -> String {
        let secs = secs_sql("join_time", "leave_time");
        let world_ref = crate::data::photo_query::WORLD_REF;
        let joins = crate::data::photo_query::FROM_JOINS;
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
}
