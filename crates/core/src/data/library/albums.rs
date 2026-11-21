//! Albums and the photos in them.
use super::*;

impl LibraryService<'_> {
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
        let name = non_empty(NameKind::Album, name)?;
        self.conn.execute(
            "INSERT INTO albums(name, description, created_at) VALUES (?1, ?2, ?3)",
            params![name, description.trim(), db::ts(now)],
        )?;
        let id = self.conn.last_insert_rowid();
        Ok(self.album(id)?.expect("album was just inserted"))
    }

    pub fn update_album(&self, id: i64, name: &str, description: &str, cover_photo_id: Option<i64>) -> Result<()> {
        let name = non_empty(NameKind::Album, name)?;
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
        let added = Self::insert_photos(&tx, album_id, photo_ids)?;
        tx.commit()?;
        Ok(added)
    }

    /// The body of [`Self::add_photos`], for a connection that is already inside a transaction.
    fn insert_photos(tx: &Connection, album_id: i64, photo_ids: &[i64]) -> Result<usize> {
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
        Ok(added)
    }

    /// Creates an album named `name` holding every photo of the visit and returns it (with its
    /// photo count and cover). All or nothing: a visit without photos, a blank name or any
    /// failure while adding leaves no album behind.
    pub fn create_album_from_visit(&self, visit_id: i64, name: &str, now: NaiveDateTime) -> Result<AlbumRow> {
        let ids: Vec<i64> =
            PhotoService::new(self.conn).photos_for_visit(visit_id)?.into_iter().map(|p| p.id).collect();
        if ids.is_empty() {
            return Err(LibraryError::NoPhotosInVisit.into());
        }
        let tx = self.conn.unchecked_transaction()?;
        let album = {
            let lib = LibraryService::new(&tx);
            let album = lib.create_album(name, "", now)?;
            Self::insert_photos(&tx, album.id, &ids)?;
            // Re-read so the returned row carries the photo count and cover.
            lib.album(album.id)?.unwrap_or(album)
        };
        tx.commit()?;
        Ok(album)
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
