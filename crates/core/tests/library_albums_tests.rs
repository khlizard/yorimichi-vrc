mod common;

use common::*;
use vrclogger_core::data::LibraryService;

fn setup() -> (Fixture, [i64; 4]) {
    let fx = Fixture::new();
    let ids = [
        add_photo(&fx, "a.png", "2026-06-01 10:00:00"),
        add_photo(&fx, "b.png", "2026-06-02 10:00:00"),
        add_photo(&fx, "c.png", "2026-06-03 10:00:00"),
        add_photo(&fx, "d.png", "2026-06-04 10:00:00"),
    ];
    (fx, ids)
}

fn order(lib: &LibraryService, album: i64) -> Vec<i64> {
    lib.album_photos(album).unwrap().iter().map(|p| p.id).collect()
}

#[test]
fn create_update_delete_cascades() {
    let (fx, [a, b, ..]) = setup();
    let lib = LibraryService::new(&fx.conn);
    let al = lib.create_album("  Trip ", " desc ", dt(2026, 7, 1, 8, 0, 0)).unwrap();
    assert_eq!((al.name.as_str(), al.description.as_str(), al.photo_count), ("Trip", "desc", 0));
    assert_eq!(al.created_at, "2026-07-01 08:00:00");
    assert_eq!(al.cover_photo_id, None);
    assert!(lib.create_album("  ", "", dt(2026, 7, 1, 8, 0, 0)).is_err());

    let newer = lib.create_album("Newer", "", dt(2026, 7, 2, 8, 0, 0)).unwrap();
    assert_eq!(lib.albums().unwrap().iter().map(|a| a.id).collect::<Vec<_>>(), [newer.id, al.id]);

    lib.update_album(al.id, "Renamed", "new", Some(b)).unwrap();
    lib.add_photos(al.id, &[a, b]).unwrap();
    let got = lib.album(al.id).unwrap().unwrap();
    assert_eq!((got.name.as_str(), got.description.as_str()), ("Renamed", "new"));
    assert_eq!(got.cover_photo_id, Some(b));
    assert_eq!(got.photo_count, 2);
    assert_eq!(got.first_taken.as_deref(), Some("2026-06-01 10:00:00"));
    assert_eq!(got.last_taken.as_deref(), Some("2026-06-02 10:00:00"));
    assert!(lib.update_album(al.id, "", "", None).is_err());

    lib.delete_album(al.id).unwrap();
    assert!(lib.album(al.id).unwrap().is_none());
    assert_eq!(fx.count("SELECT count(*) FROM album_photos"), 0);
    assert_eq!(fx.count("SELECT count(*) FROM photos"), 4); // photos are untouched
    assert_eq!(lib.albums().unwrap().len(), 1);
}

#[test]
fn add_skips_duplicates_and_unknown_ids() {
    let (fx, [a, b, c, _]) = setup();
    let lib = LibraryService::new(&fx.conn);
    let al = lib.create_album("A", "", dt(2026, 7, 1, 0, 0, 0)).unwrap();

    assert_eq!(lib.add_photos(al.id, &[b, a, b, 999_999]).unwrap(), 2);
    assert_eq!(order(&lib, al.id), [b, a]);
    assert_eq!(lib.add_photos(al.id, &[c, a]).unwrap(), 1);
    assert_eq!(order(&lib, al.id), [b, a, c]);
    assert_eq!(lib.add_photos(al.id, &[]).unwrap(), 0);
    assert_eq!(lib.album(al.id).unwrap().unwrap().photo_count, 3);
    assert_eq!(lib.albums_of_photo(a).unwrap(), [al.id]);

    let other = lib.create_album("B", "", dt(2026, 7, 2, 0, 0, 0)).unwrap();
    lib.add_photos(other.id, &[a]).unwrap();
    assert_eq!(lib.albums_of_photo(a).unwrap(), [al.id, other.id]);
    assert!(lib.albums_of_photo(c).unwrap() == [al.id]);
}

#[test]
fn reorder_with_partial_list_and_remove() {
    let (fx, [a, b, c, d]) = setup();
    let lib = LibraryService::new(&fx.conn);
    let al = lib.create_album("A", "", dt(2026, 7, 1, 0, 0, 0)).unwrap();
    lib.add_photos(al.id, &[a, b, c, d]).unwrap();

    lib.reorder(al.id, &[c]).unwrap();
    assert_eq!(order(&lib, al.id), [c, a, b, d]); // unlisted keep their relative order
    lib.reorder(al.id, &[d, 999_999, b, d]).unwrap(); // unknown ids and repeats are ignored
    assert_eq!(order(&lib, al.id), [d, b, c, a]);

    lib.remove_photos(al.id, &[b, 999_999]).unwrap();
    assert_eq!(order(&lib, al.id), [d, c, a]);
    // appended photos go after the current maximum position
    lib.add_photos(al.id, &[b]).unwrap();
    assert_eq!(order(&lib, al.id), [d, c, a, b]);
}

#[test]
fn cover_falls_back_to_first_photo() {
    let (fx, [a, b, c, _]) = setup();
    let lib = LibraryService::new(&fx.conn);
    let al = lib.create_album("A", "", dt(2026, 7, 1, 0, 0, 0)).unwrap();
    lib.add_photos(al.id, &[b, a, c]).unwrap();
    assert_eq!(lib.album(al.id).unwrap().unwrap().cover_photo_id, Some(b)); // first by position

    lib.update_album(al.id, "A", "", Some(c)).unwrap();
    assert_eq!(lib.album(al.id).unwrap().unwrap().cover_photo_id, Some(c));

    // cover removed from the album
    lib.remove_photos(al.id, &[c]).unwrap();
    assert_eq!(lib.album(al.id).unwrap().unwrap().cover_photo_id, Some(b));

    // cover file went missing: the next present photo is used, and album_photos still lists it
    lib.update_album(al.id, "A", "", Some(a)).unwrap();
    fx.conn.execute("UPDATE photos SET missing = 1 WHERE id = ?1", [a]).unwrap();
    assert_eq!(lib.album(al.id).unwrap().unwrap().cover_photo_id, Some(b));
    fx.conn.execute("UPDATE photos SET missing = 1 WHERE id = ?1", [b]).unwrap();
    let got = lib.album(al.id).unwrap().unwrap();
    assert_eq!((got.cover_photo_id, got.photo_count), (None, 2));
    let photos = lib.album_photos(al.id).unwrap();
    assert_eq!(photos.len(), 2);
    assert!(photos.iter().all(|p| p.missing));
}

#[test]
fn deleting_a_photo_row_leaves_the_album_consistent() {
    let (fx, [a, b, ..]) = setup();
    let lib = LibraryService::new(&fx.conn);
    let al = lib.create_album("A", "", dt(2026, 7, 1, 0, 0, 0)).unwrap();
    lib.add_photos(al.id, &[a, b]).unwrap();
    fx.conn.execute("DELETE FROM photos WHERE id = ?1", [a]).unwrap();
    assert_eq!(order(&lib, al.id), [b]);
}
