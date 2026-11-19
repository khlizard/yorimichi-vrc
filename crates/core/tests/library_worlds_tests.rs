mod common;

use common::*;
use vrclogger_core::data::library_dtos::{WorldRow, WorldSort};
use vrclogger_core::data::{LibraryService, QueryService};

const W1: &str = "wrld_aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa";
const W2: &str = "wrld_bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb";
const W3: &str = "wrld_cccccccc-cccc-cccc-cccc-cccccccccccc";
const W4: &str = "wrld_dddddddd-dddd-dddd-dddd-dddddddddddd";

fn setup() -> Fixture {
    let fx = Fixture::new();
    import_visits(
        &fx,
        &[
            V::new("2026.06.01", "21:00:00", "22:00:00", "World One", W1).with("Friend A", USR_A, "21:10:00", "21:40:00"),
            V::new("2026.06.05", "21:00:00", "21:30:00", "World One", W1),
            V::new("2026.06.20", "10:00:00", "11:00:00", "World Two", W2),
            V::new("2026.06.25", "10:00:00", "10:10:00", "100% Fun", W3),
            V::new("2026.06.26", "10:00:00", "10:05:00", "100 Fun", W4),
        ],
    );
    fx
}

fn names(rows: &[WorldRow]) -> Vec<&str> {
    rows.iter().map(|w| w.name.as_str()).collect()
}

#[test]
fn counts_time_and_sort_orders() {
    let fx = setup();
    let lib = LibraryService::new(&fx.conn);

    let all = lib.worlds(None, false, WorldSort::default()).unwrap();
    assert_eq!(names(&all), ["100 Fun", "100% Fun", "World Two", "World One"]);
    let one = all.iter().find(|w| w.name == "World One").unwrap();
    assert_eq!(one.world_id, W1);
    assert_eq!(one.visit_count, 2);
    assert_eq!(one.total_seconds, 3600 + 1800);
    assert_eq!(one.first_visit.as_deref(), Some("2026-06-01 21:00:00"));
    assert_eq!(one.last_visit.as_deref(), Some("2026-06-05 21:00:00"));
    assert_eq!((one.memo.as_str(), one.favorite, one.photo_count), ("", false, 0));

    let by_count = lib.worlds(None, false, WorldSort::VisitCount).unwrap();
    assert_eq!(names(&by_count), ["World One", "100 Fun", "100% Fun", "World Two"]);
    let by_time = lib.worlds(None, false, WorldSort::TotalTime).unwrap();
    assert_eq!(names(&by_time), ["World One", "World Two", "100% Fun", "100 Fun"]);
    let by_name = lib.worlds(None, false, WorldSort::Name).unwrap();
    assert_eq!(names(&by_name), ["100 Fun", "100% Fun", "World One", "World Two"]);

    assert_eq!(lib.world(one.id).unwrap().unwrap().visit_count, 2);
    assert!(lib.world(999_999).unwrap().is_none());
}

#[test]
fn sort_deserializes_from_camel_case() {
    let s: WorldSort = serde_json::from_str("\"totalTime\"").unwrap();
    assert_eq!(s, WorldSort::TotalTime);
}

#[test]
fn term_search_matches_name_and_memo_and_escapes_like() {
    let fx = setup();
    let lib = LibraryService::new(&fx.conn);
    let w2 = world_ref(&fx, W2);
    lib.set_world_note(w2, "great sunset view", false, dt(2026, 7, 1, 0, 0, 0)).unwrap();

    assert_eq!(names(&lib.worlds(Some("sunset"), false, WorldSort::Name).unwrap()), ["World Two"]);
    assert_eq!(names(&lib.worlds(Some("two"), false, WorldSort::Name).unwrap()), ["World Two"]);
    assert_eq!(names(&lib.worlds(Some("100"), false, WorldSort::Name).unwrap()), ["100 Fun", "100% Fun"]);
    // a literal percent sign, not a wildcard
    assert_eq!(names(&lib.worlds(Some("%"), false, WorldSort::Name).unwrap()), ["100% Fun"]);
    assert_eq!(names(&lib.worlds(Some("100%"), false, WorldSort::Name).unwrap()), ["100% Fun"]);
    assert_eq!(lib.worlds(Some("   "), false, WorldSort::Name).unwrap().len(), 4);
}

#[test]
fn favorite_filter_and_note_lifecycle() {
    let fx = setup();
    let lib = LibraryService::new(&fx.conn);
    let w1 = world_ref(&fx, W1);
    lib.set_world_note(w1, "", true, dt(2026, 7, 1, 0, 0, 0)).unwrap();
    let fav = lib.worlds(None, true, WorldSort::Name).unwrap();
    assert_eq!(names(&fav), ["World One"]);
    assert!(fav[0].favorite);

    lib.set_world_note(w1, "  memo  ", true, dt(2026, 7, 2, 0, 0, 0)).unwrap();
    assert_eq!(fx.count("SELECT count(*) FROM world_notes"), 1);
    assert_eq!(lib.world(w1).unwrap().unwrap().memo, "memo");

    lib.set_world_note(w1, "", false, dt(2026, 7, 3, 0, 0, 0)).unwrap();
    assert_eq!(fx.count("SELECT count(*) FROM world_notes"), 0);
    assert!(lib.worlds(None, true, WorldSort::Name).unwrap().is_empty());
}

#[test]
fn photo_count_via_visit_and_xmp_fallback() {
    let fx = setup();
    add_photo(&fx, "in-visit.png", "2026-06-01 21:30:00");
    let gone = add_photo(&fx, "gone.png", "2026-06-01 21:31:00");
    fx.conn.execute("UPDATE photos SET missing = 1 WHERE id = ?1", [gone]).unwrap();
    let loose = add_photo(&fx, "loose.png", "2026-07-01 12:00:00");
    fx.conn.execute("UPDATE photos SET xmp_world_id = ?1 WHERE id = ?2", rusqlite::params![W2, loose]).unwrap();
    let unknown = add_photo(&fx, "unknown.png", "2026-07-02 12:00:00");
    fx.conn.execute("UPDATE photos SET xmp_world_id = 'wrld_unknown' WHERE id = ?1", [unknown]).unwrap();

    let lib = LibraryService::new(&fx.conn);
    let rows = lib.worlds(None, false, WorldSort::Name).unwrap();
    let count = |n: &str| rows.iter().find(|w| w.name == n).unwrap().photo_count;
    assert_eq!(count("World One"), 1);
    assert_eq!(count("World Two"), 1);
    assert_eq!(count("100% Fun"), 0);
}

#[test]
fn visit_rows_carry_world_and_user_ids() {
    let fx = setup();
    let q = QueryService::new(&fx.conn);
    let visits = q.get_visits(Some("World One"), None, None).unwrap();
    assert_eq!(visits.len(), 2);
    assert!(visits.iter().all(|v| v.world_id.as_deref() == Some(W1)));
    assert!(visits.iter().all(|v| v.world_ref_id == Some(world_ref(&fx, W1))));
    let json = serde_json::to_value(&visits[0]).unwrap();
    assert_eq!(json["worldId"], W1);

    let first = visits.iter().find(|v| v.join_time == "2026-06-01 21:00:00").unwrap();
    let players = q.get_visit_players(first.id).unwrap();
    let friend = players.iter().find(|p| !p.is_local).unwrap();
    assert_eq!(friend.user_id, USR_A);
    assert!(players.iter().any(|p| p.is_local && p.user_id == LOCAL_USR));
}
