mod common;

use std::path::{Path, PathBuf};

use common::{add_photo, d, log, Fixture};
use yorimichi_core::data::photo_query::{PhotoFilter, PhotoService};
use yorimichi_core::data::{db, ImportService};
use yorimichi_core::photos::{relink_visits, scan_folders};

const FRIEND: &str = "usr_11111111-1111-1111-1111-111111111111";
const WORLD: &str = "wrld_11111111-1111-1111-1111-111111111111";

fn write_png(path: &Path, w: u32, h: u32) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    image::RgbImage::new(w, h).save(path).unwrap();
}

/// VRChat-style name so `taken_at` comes from the file name.
fn photo(dir: &Path, stamp: &str, tag: &str) -> PathBuf {
    let p = dir.join(format!("VRChat_{stamp}.000_{tag}.png"));
    write_png(&p, 4, 3);
    p
}

fn visit_log(day: &str, instance: u32) -> String {
    let d = day.replace('-', ".");
    log(&format!(
        r"
        {d} 21:00:00 {{P}}User Authenticated: Tester (usr_22222222-2222-2222-2222-222222222222)
        {d} 21:00:05 {{P}}[Behaviour] Entering Room: Sample World
        {d} 21:00:05 {{P}}[Behaviour] Joining {WORLD}:{instance}
        {d} 21:00:10 {{P}}[Behaviour] OnPlayerJoined Tester (usr_22222222-2222-2222-2222-222222222222)
        {d} 21:10:00 {{P}}[Behaviour] OnPlayerJoined Friend ({FRIEND})
        {d} 21:20:00 {{P}}[Behaviour] OnPlayerLeft Friend ({FRIEND})
        {d} 22:00:00 {{P}}[Behaviour] OnLeftRoom
        "
    ))
}

fn scan(fx: &Fixture, folders: &[&Path]) -> yorimichi_core::photos::ScanSummary {
    let v: Vec<PathBuf> = folders.iter().map(|p| p.to_path_buf()).collect();
    scan_folders(&fx.conn, &v, None).unwrap()
}

fn photos_dir(fx: &Fixture) -> PathBuf {
    fx.dir.path().join("photos")
}

#[test]
fn scan_adds_recursively_and_ignores_other_extensions() {
    let fx = Fixture::new();
    let root = photos_dir(&fx);
    photo(&root, "2026-06-23_21-05-00", "a");
    photo(&root.join("sub").join("deeper"), "2026-06-23_21-06-00", "b");
    write_png(&root.join("UPPER.PNG"), 2, 2);
    std::fs::write(root.join("notes.txt"), "x").unwrap();
    std::fs::write(root.join("movie.mp4"), "x").unwrap();

    let s = scan(&fx, &[&root]);
    assert_eq!((s.scanned, s.added, s.updated, s.missing), (3, 3, 0, 0));
    assert_eq!(fx.count("SELECT count(*) FROM photos"), 3);
    let (w, h): (i64, i64) = fx
        .conn
        .query_row("SELECT width, height FROM photos WHERE file_name LIKE 'VRChat%a.png'", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap();
    assert_eq!((w, h), (4, 3));
    let src: String = fx
        .conn
        .query_row("SELECT time_source FROM photos WHERE file_name LIKE 'VRChat%a.png'", [], |r| r.get(0))
        .unwrap();
    assert_eq!(src, "name");
    let src: String = fx
        .conn
        .query_row("SELECT time_source FROM photos WHERE file_name = 'UPPER.PNG'", [], |r| r.get(0))
        .unwrap();
    assert_eq!(src, "mtime");
}

#[test]
fn rescan_without_changes_does_nothing() {
    let fx = Fixture::new();
    let root = photos_dir(&fx);
    photo(&root, "2026-06-23_21-05-00", "a");
    photo(&root, "2026-06-23_21-06-00", "b");
    scan(&fx, &[&root]);
    let s = scan(&fx, &[&root]);
    assert_eq!((s.scanned, s.added, s.updated, s.missing), (2, 0, 0, 0));
}

#[test]
fn changed_file_updates_but_keeps_id_and_favorite() {
    let fx = Fixture::new();
    let root = photos_dir(&fx);
    let p = photo(&root, "2026-06-23_21-05-00", "a");
    scan(&fx, &[&root]);
    let id: i64 = fx.conn.query_row("SELECT id FROM photos", [], |r| r.get(0)).unwrap();
    PhotoService::new(&fx.conn).set_favorite(id, true).unwrap();

    write_png(&p, 10, 8); // different size
    let s = scan(&fx, &[&root]);
    assert_eq!((s.added, s.updated), (0, 1));
    let (id2, fav, w): (i64, bool, i64) = fx
        .conn
        .query_row("SELECT id, favorite, width FROM photos", [], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap();
    assert_eq!((id2, fav, w), (id, true, 10));
}

#[test]
fn deleted_file_is_marked_missing_and_can_return() {
    let fx = Fixture::new();
    let root = photos_dir(&fx);
    let p = photo(&root, "2026-06-23_21-05-00", "a");
    photo(&root, "2026-06-23_21-06-00", "b");
    scan(&fx, &[&root]);

    std::fs::remove_file(&p).unwrap();
    let s = scan(&fx, &[&root]);
    assert_eq!(s.missing, 1);
    assert_eq!(fx.count("SELECT count(*) FROM photos"), 2); // never deleted
    assert_eq!(fx.count("SELECT count(*) FROM photos WHERE missing = 1"), 1);
    let svc = PhotoService::new(&fx.conn);
    assert_eq!(svc.count().unwrap(), 1);
    assert_eq!(svc.photos(&PhotoFilter::default(), None, 100).unwrap().len(), 1);
    let with_missing = PhotoFilter { include_missing: true, ..Default::default() };
    assert_eq!(svc.photos(&with_missing, None, 100).unwrap().len(), 2);
    assert_eq!(scan(&fx, &[&root]).missing, 0); // already flagged: not counted again

    write_png(&p, 4, 3);
    scan(&fx, &[&root]);
    assert_eq!(fx.count("SELECT count(*) FROM photos WHERE missing = 1"), 0);
    assert_eq!(fx.count("SELECT count(*) FROM photos"), 2);
}

#[test]
fn rows_under_unavailable_folder_are_left_alone() {
    let fx = Fixture::new();
    let a = fx.dir.path().join("a");
    let b = fx.dir.path().join("b");
    photo(&a, "2026-06-23_21-05-00", "a");
    photo(&b, "2026-06-23_21-06-00", "b");
    scan(&fx, &[&a, &b]);

    std::fs::remove_dir_all(&b).unwrap(); // e.g. unplugged drive
    let s = scan(&fx, &[&a, &b]);
    assert_eq!(s.missing, 0);
    assert_eq!(fx.count("SELECT count(*) FROM photos WHERE missing = 1"), 0);

    // Once the folder is no longer configured its rows are outside every folder.
    let s = scan(&fx, &[&a]);
    assert_eq!(s.missing, 1);
}

#[test]
fn relink_links_inside_visit_and_survives_reimport() {
    let fx = Fixture::new();
    let root = photos_dir(&fx);
    photo(&root, "2026-06-23_21-05-00", "in");
    photo(&root, "2026-06-23_23-30-00", "out");

    let svc = ImportService::new(&fx.conn);
    let log_a = fx.write_log("output_log_2026-06-22_21-00-00.txt", &visit_log("2026-06-22", 1));
    let log_b = fx.write_log("output_log_2026-06-23_21-00-00.txt", &visit_log("2026-06-23", 2));
    // The photo's visit is imported first so a re-import cannot reuse its id.
    svc.import_file(&log_b, None).unwrap();
    svc.import_file(&log_a, None).unwrap();
    scan(&fx, &[&root]);

    let visit_of = |tag: &str| -> Option<i64> {
        fx.conn
            .query_row(&format!("SELECT visit_id FROM photos WHERE file_name LIKE '%{tag}.png'"), [], |r| r.get(0))
            .unwrap()
    };
    let current_visit: i64 =
        fx.conn.query_row("SELECT id FROM visits WHERE join_time LIKE '2026-06-23%'", [], |r| r.get(0)).unwrap();
    assert_eq!(visit_of("in"), Some(current_visit));
    assert_eq!(visit_of("out"), None);
    assert_eq!(relink_visits(&fx.conn).unwrap(), 0); // idempotent

    // Re-import replaces the visit rows with new ids.
    svc.mark_all_for_reimport().unwrap();
    svc.import_file(&log_b, None).unwrap();
    let new_visit: i64 =
        fx.conn.query_row("SELECT id FROM visits WHERE join_time LIKE '2026-06-23%'", [], |r| r.get(0)).unwrap();
    assert_ne!(new_visit, current_visit);
    assert_eq!(relink_visits(&fx.conn).unwrap(), 1);
    assert_eq!(visit_of("in"), Some(new_visit));
    assert_eq!(visit_of("out"), None);
}

#[test]
fn upgrade_retimes_photos_whose_xmp_date_is_far_from_the_name() {
    let fx = Fixture::new();
    let svc = ImportService::new(&fx.conn);
    let log_path = fx.write_log("output_log_2026-06-23_21-00-00.txt", &visit_log("2026-06-23", 1));
    svc.import_file(&log_path, None).unwrap();

    // Rows as an older version stored them: the time taken from the XMP as is.
    let print = add_photo(&fx, "vrc_2026-06-23_21-30-00.147_2048x1440.jpg", "2026-06-24 06:29:30");
    let camera = add_photo(&fx, "VRChat_2026-06-23_21-40-00.000_3840x2160.png", "2026-06-23 21:40:02");
    let no_name = add_photo(&fx, "edited.png", "2026-06-23 21:50:00");
    fx.conn.execute("UPDATE photos SET time_source = 'xmp'", []).unwrap();
    let visit: i64 = fx.conn.query_row("SELECT id FROM visits", [], |r| r.get(0)).unwrap();
    let row = |id: i64| -> (String, String, Option<i64>) {
        fx.conn
            .query_row("SELECT taken_at, time_source, visit_id FROM photos WHERE id = ?1", [id], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })
            .unwrap()
    };
    assert_eq!(row(print).2, None);

    fx.conn.pragma_update(None, "user_version", 1).unwrap();
    db::ensure_created(&fx.conn).unwrap();
    assert_eq!(db::schema_version(&fx.conn).unwrap(), db::SCHEMA_VERSION);
    assert_eq!(row(print), ("2026-06-23 21:30:00".into(), "name".into(), Some(visit)));
    assert_eq!(row(camera), ("2026-06-23 21:40:02".into(), "xmp".into(), Some(visit)));
    assert_eq!(row(no_name), ("2026-06-23 21:50:00".into(), "xmp".into(), Some(visit)));
}

/// Imports one visit on 2026-06-23 (21:00-22:00, friend present 21:10-21:20) and returns the photo dir.
fn setup_visit_photos(fx: &Fixture) -> PathBuf {
    let root = photos_dir(fx);
    photo(&root, "2026-06-23_21-05-00", "alone");
    photo(&root, "2026-06-23_21-15-00", "friend");
    photo(&root, "2026-06-23_21-30-00", "after");
    photo(&root, "2026-06-23_23-30-00", "outside");
    photo(&root, "2026-06-24_03-00-00", "late");
    let lg = fx.write_log("output_log_2026-06-23_21-00-00.txt", &visit_log("2026-06-23", 1));
    ImportService::new(&fx.conn).import_file(&lg, None).unwrap();
    scan(fx, &[&root]);
    root
}

fn names(rows: &[yorimichi_core::data::photo_query::PhotoRow]) -> Vec<String> {
    rows.iter().map(|r| r.file_name.rsplit('_').next().unwrap().trim_end_matches(".png").to_string()).collect()
}

#[test]
fn paging_has_no_gaps_or_duplicates() {
    let fx = Fixture::new();
    let root = photos_dir(&fx);
    for i in 0..12 {
        // Several photos share the same second so the id tiebreak matters.
        photo(&root, &format!("2026-06-23_22-{:02}-00", i / 3), &format!("{i:02}"));
    }
    scan(&fx, &[&root]);
    let svc = PhotoService::new(&fx.conn);
    let f = PhotoFilter::default();
    let all = svc.photos(&f, None, 1000).unwrap();
    assert_eq!(all.len(), 12);

    let mut paged = Vec::new();
    let mut before = None;
    loop {
        let page = svc.photos(&f, before.clone(), 5).unwrap();
        if page.is_empty() {
            break;
        }
        let last = page.last().unwrap();
        before = Some((last.taken_at.clone(), last.id));
        paged.extend(page);
    }
    let ids = |v: &[yorimichi_core::data::photo_query::PhotoRow]| v.iter().map(|r| r.id).collect::<Vec<_>>();
    assert_eq!(ids(&paged), ids(&all));
    assert!(all.windows(2).all(|w| (&w[0].taken_at, w[0].id) > (&w[1].taken_at, w[1].id)));
}

#[test]
fn filters() {
    let fx = Fixture::new();
    setup_visit_photos(&fx);
    let svc = PhotoService::new(&fx.conn);
    let all = svc.photos(&PhotoFilter::default(), None, 100).unwrap();
    assert_eq!(all.len(), 5);

    // favorite
    let fav = all.iter().find(|p| p.file_name.ends_with("_after.png")).unwrap();
    svc.set_favorite(fav.id, true).unwrap();
    let f = PhotoFilter { favorite_only: true, ..Default::default() };
    assert_eq!(names(&svc.photos(&f, None, 100).unwrap()), ["after"]);

    // visit
    let visit_id: i64 = fx.conn.query_row("SELECT id FROM visits", [], |r| r.get(0)).unwrap();
    let f = PhotoFilter { visit_id: Some(visit_id), ..Default::default() };
    assert_eq!(svc.photos(&f, None, 100).unwrap().len(), 3);
    assert_eq!(names(&svc.photos_for_visit(visit_id).unwrap()), ["alone", "friend", "after"]); // ascending

    // player present at taken_at
    let friend: i64 =
        fx.conn.query_row("SELECT id FROM players WHERE user_id = ?1", [FRIEND], |r| r.get(0)).unwrap();
    let f = PhotoFilter { player_id: Some(friend), ..Default::default() };
    assert_eq!(names(&svc.photos(&f, None, 100).unwrap()), ["friend"]);

    // world: visit's world, and the XMP fallback for a photo without a visit
    let world_ref: i64 =
        fx.conn.query_row("SELECT id FROM worlds WHERE world_id = ?1", [WORLD], |r| r.get(0)).unwrap();
    let f = PhotoFilter { world_ref_id: Some(world_ref), ..Default::default() };
    assert_eq!(svc.photos(&f, None, 100).unwrap().len(), 3);
    fx.conn
        .execute(
            "UPDATE photos SET xmp_world_id = ?1, xmp_world_name = 'From XMP' WHERE file_name LIKE '%outside.png'",
            [WORLD],
        )
        .unwrap();
    let rows = svc.photos(&f, None, 100).unwrap();
    assert_eq!(rows.len(), 4);
    let outside = rows.iter().find(|p| p.file_name.ends_with("outside.png")).unwrap();
    assert_eq!(outside.visit_id, None);
    assert_eq!(outside.world_name.as_deref(), Some("From XMP"));
    assert_eq!(outside.world_ref_id, Some(world_ref));
    let inside = rows.iter().find(|p| p.file_name.ends_with("alone.png")).unwrap();
    assert_eq!(inside.world_name.as_deref(), Some("Sample World"));

    // dates are inclusive calendar days
    let f = PhotoFilter { from: Some(d(2026, 6, 24)), to: Some(d(2026, 6, 24)), ..Default::default() };
    assert_eq!(names(&svc.photos(&f, None, 100).unwrap()), ["late"]);
}

#[test]
fn days_use_logical_day_start() {
    let fx = Fixture::new();
    setup_visit_photos(&fx);
    let svc = PhotoService::new(&fx.conn);
    let f = PhotoFilter::default();

    let days = svc.photo_days(&f, 6).unwrap();
    assert_eq!(days.len(), 1);
    assert_eq!((days[0].day.as_str(), days[0].count), ("2026-06-23", 5));

    let days = svc.photo_days(&f, 0).unwrap();
    let got: Vec<(&str, i64)> = days.iter().map(|d| (d.day.as_str(), d.count)).collect();
    assert_eq!(got, [("2026-06-24", 1), ("2026-06-23", 4)]);
}

#[test]
fn detail_lists_only_players_present_at_taken_at() {
    let fx = Fixture::new();
    setup_visit_photos(&fx);
    let svc = PhotoService::new(&fx.conn);
    let id_of = |tag: &str| -> i64 {
        fx.conn
            .query_row(&format!("SELECT id FROM photos WHERE file_name LIKE '%{tag}.png'"), [], |r| r.get(0))
            .unwrap()
    };

    let det = svc.photo_detail(id_of("friend")).unwrap().unwrap();
    let who: Vec<(&str, bool)> = det.players.iter().map(|p| (p.display_name.as_str(), p.is_local)).collect();
    assert_eq!(who, [("Tester", true), ("Friend", false)]); // local first
    assert_eq!(det.players[1].user_id, FRIEND);
    assert_eq!(det.world_id.as_deref(), Some(WORLD));
    assert_eq!(det.instance_id.as_deref(), Some("1"));
    assert_eq!(det.location.as_deref(), Some(format!("{WORLD}:1").as_str()));
    assert!(det.visit_join_time.is_some() && det.visit_leave_time.is_some());

    let det = svc.photo_detail(id_of("alone")).unwrap().unwrap();
    assert_eq!(det.players.len(), 1);
    let det = svc.photo_detail(id_of("after")).unwrap().unwrap();
    assert_eq!(det.players.len(), 1);

    let det = svc.photo_detail(id_of("outside")).unwrap().unwrap();
    assert!(det.players.is_empty() && det.photo.visit_id.is_none() && det.world_id.is_none());

    assert!(svc.photo_detail(999_999).unwrap().is_none());
    assert!(svc.photo_path(999_999).unwrap().is_none());
    let path = svc.photo_path(id_of("alone")).unwrap().unwrap();
    assert!(path.ends_with("_alone.png"));
}

#[test]
fn newest_ids_for_thumbnail_pregeneration() {
    let fx = Fixture::new();
    setup_visit_photos(&fx);
    let svc = PhotoService::new(&fx.conn);
    assert_eq!(svc.count().unwrap(), 5);
    let first = svc.photo_ids_newest(2, 0).unwrap();
    assert_eq!(first.len(), 2);
    assert!(first[0].1.ends_with("_late.png"));
    assert!(!first[0].2);
    let rest = svc.photo_ids_newest(10, 2).unwrap();
    assert_eq!(rest.len(), 3);
    assert_ne!(first[1].0, rest[0].0);
}
