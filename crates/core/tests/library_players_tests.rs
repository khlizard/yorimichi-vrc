mod common;

use common::*;
use vrclogger_core::data::{ImportService, LibraryService};

const W1: &str = "wrld_aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa";
const W2: &str = "wrld_bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb";

fn setup() -> Fixture {
    let fx = Fixture::new();
    import_visits(
        &fx,
        &[
            V::new("2026.06.01", "21:00:00", "22:00:00", "World One", W1)
                .with("Friend A", USR_A, "21:10:00", "21:40:00")
                .with("Friend B", USR_B, "21:20:00", "21:50:00"),
            V::new("2026.06.05", "21:00:00", "21:30:00", "World One", W1).with("Friend A", USR_A, "21:05:00", "21:25:00"),
            V::new("2026.06.20", "10:00:00", "11:00:00", "World Two", W2)
                .with("Friend A", USR_A, "10:05:00", "10:35:00")
                .with("Friend B", USR_B, "10:05:00", "10:35:00"),
        ],
    );
    fx
}

fn visit_id(fx: &Fixture, join_prefix: &str) -> i64 {
    fx.conn
        .query_row("SELECT id FROM visits WHERE join_time LIKE ?1", [format!("{join_prefix}%")], |r| r.get(0))
        .unwrap()
}

fn visit_ids(fx: &Fixture) -> Vec<i64> {
    let mut st = fx.conn.prepare("SELECT id FROM visits ORDER BY id").unwrap();
    st.query_map([], |r| r.get(0)).unwrap().map(|r| r.unwrap()).collect()
}

#[test]
fn tags_crud_and_duplicate_name() {
    let fx = Fixture::new();
    let lib = LibraryService::new(&fx.conn);
    let t = lib.create_tag("  Friends ", "#FF0000").unwrap();
    assert_eq!((t.name.as_str(), t.color.as_str()), ("Friends", "#ff0000"));
    let dup = lib.create_tag("Friends", "#00ff00").unwrap();
    assert_eq!(dup.id, t.id);
    assert_eq!(dup.color, "#ff0000"); // existing tag is returned unchanged
    let bad = lib.create_tag("Bad", "red").unwrap();
    assert_eq!(bad.color, "#6b7280");
    assert!(lib.create_tag("   ", "#000000").is_err());

    lib.update_tag(bad.id, "Alpha", "#112233").unwrap();
    let tags = lib.list_tags().unwrap();
    assert_eq!(tags.iter().map(|t| t.tag.name.as_str()).collect::<Vec<_>>(), ["Alpha", "Friends"]);
    assert_eq!(tags[0].tag.color, "#112233");

    let json = serde_json::to_value(&tags[0]).unwrap();
    assert_eq!(json["name"], "Alpha");
    assert_eq!(json["playerCount"], 0);

    lib.delete_tag(t.id).unwrap();
    assert_eq!(lib.list_tags().unwrap().len(), 1);
}

#[test]
fn set_player_tags_replaces_and_players_by_tag() {
    let fx = setup();
    let lib = LibraryService::new(&fx.conn);
    let (a, b) = (player_id(&fx, USR_A), player_id(&fx, USR_B));
    let t1 = lib.create_tag("One", "#111111").unwrap();
    let t2 = lib.create_tag("Two", "#222222").unwrap();

    lib.set_player_tags(a, &[t1.id, t2.id]).unwrap();
    lib.set_player_tags(b, &[t1.id]).unwrap();
    let counts = lib.list_tags().unwrap();
    assert_eq!(counts.iter().map(|c| c.player_count).collect::<Vec<_>>(), [2, 1]);

    let hits = lib.players_by_tag(t1.id).unwrap();
    // most encounters first: A has 3, B has 2
    assert_eq!(hits.iter().map(|h| h.id).collect::<Vec<_>>(), [a, b]);
    assert_eq!(hits[0].encounter_count, 3);

    lib.set_player_tags(a, &[t2.id, 99999]).unwrap(); // replace; unknown id ignored
    assert_eq!(lib.players_by_tag(t1.id).unwrap().len(), 1);
    assert_eq!(lib.player_profile(a).unwrap().unwrap().tags.len(), 1);
    lib.delete_tag(t2.id).unwrap();
    assert!(lib.player_profile(a).unwrap().unwrap().tags.is_empty());
}

#[test]
fn memo_set_and_clear() {
    let fx = setup();
    let lib = LibraryService::new(&fx.conn);
    let a = player_id(&fx, USR_A);
    lib.set_player_memo(a, "  met at a meetup \n", dt(2026, 7, 1, 1, 2, 3)).unwrap();
    assert_eq!(lib.player_profile(a).unwrap().unwrap().memo, "met at a meetup");
    lib.set_player_memo(a, "changed", dt(2026, 7, 2, 1, 2, 3)).unwrap();
    assert_eq!(fx.count("SELECT count(*) FROM player_notes"), 1);
    let at: String = fx.conn.query_row("SELECT updated_at FROM player_notes", [], |r| r.get(0)).unwrap();
    assert_eq!(at, "2026-07-02 01:02:03");
    lib.set_player_memo(a, "   ", dt(2026, 7, 3, 0, 0, 0)).unwrap();
    assert_eq!(fx.count("SELECT count(*) FROM player_notes"), 0);
    assert_eq!(lib.player_profile(a).unwrap().unwrap().memo, "");
}

#[test]
fn user_data_survives_reimport() {
    let fx = setup();
    let lib = LibraryService::new(&fx.conn);
    let (a, w1) = (player_id(&fx, USR_A), world_ref(&fx, W1));
    let tag = lib.create_tag("Keep", "#abcdef").unwrap();
    lib.set_player_tags(a, &[tag.id]).unwrap();
    lib.set_player_memo(a, "remember", dt(2026, 7, 1, 0, 0, 0)).unwrap();
    lib.set_world_note(w1, "nice place", true, dt(2026, 7, 1, 0, 0, 0)).unwrap();
    let photo = add_photo(&fx, "p.png", "2026-06-01 21:30:00");
    let album = lib.create_album("Keepsake", "", dt(2026, 7, 1, 0, 0, 0)).unwrap();
    lib.add_photos(album.id, &[photo]).unwrap();
    let visits_before = fx.count("SELECT count(*) FROM visits");
    let old_visit_ids = visit_ids(&fx);

    let imp = ImportService::new(&fx.conn);
    assert!(imp.mark_all_for_reimport().unwrap() > 0);
    for i in 0..3 {
        let path = fx.dir.path().join(format!("output_log_2026-01-01_00-00-{i:02}.txt"));
        assert!(imp.import_file(&path, None).unwrap());
    }
    assert_eq!(fx.count("SELECT count(*) FROM visits"), visits_before);
    assert_ne!(old_visit_ids, visit_ids(&fx), "visit ids are expected to change on re-import");

    assert_eq!(player_id(&fx, USR_A), a);
    assert_eq!(world_ref(&fx, W1), w1);
    let p = lib.player_profile(a).unwrap().unwrap();
    assert_eq!(p.memo, "remember");
    assert_eq!(p.tags.len(), 1);
    assert_eq!(p.encounter_count, 3);
    let w = lib.world(w1).unwrap().unwrap();
    assert_eq!((w.memo.as_str(), w.favorite), ("nice place", true));
    assert_eq!(lib.album_photos(album.id).unwrap().len(), 1);
    // the photo link is re-derived from taken_at
    vrclogger_core::photos::relink_visits(&fx.conn).unwrap();
    assert_eq!(lib.player_profile(a).unwrap().unwrap().photo_count, 1);
}

#[test]
fn profile_numbers() {
    let fx = setup();
    add_photo(&fx, "p1.png", "2026-06-01 21:15:00"); // A only
    add_photo(&fx, "p2.png", "2026-06-01 21:30:00"); // A and B
    add_photo(&fx, "p3.png", "2026-06-01 21:55:00"); // nobody
    let missing = add_photo(&fx, "p4.png", "2026-06-01 21:31:00");
    fx.conn.execute("UPDATE photos SET missing = 1 WHERE id = ?1", [missing]).unwrap();
    let lib = LibraryService::new(&fx.conn);
    let (a, b) = (player_id(&fx, USR_A), player_id(&fx, USR_B));

    let pa = lib.player_profile(a).unwrap().unwrap();
    assert_eq!(pa.user_id, USR_A);
    assert!(!pa.is_local);
    assert_eq!(pa.encounter_count, 3);
    assert_eq!(pa.total_seconds, 1800 + 1200 + 1800);
    assert_eq!(pa.first_met.as_deref(), Some("2026-06-01 21:10:00"));
    assert_eq!(pa.last_met.as_deref(), Some("2026-06-20 10:05:00"));
    assert_eq!(pa.photo_count, 2);
    let pb = lib.player_profile(b).unwrap().unwrap();
    assert_eq!((pb.encounter_count, pb.total_seconds, pb.photo_count), (2, 1800 + 1800, 1));
    assert!(lib.player_profile(424242).unwrap().is_none());
}

#[test]
fn badges_first_met_and_reunion() {
    let fx = setup();
    let lib = LibraryService::new(&fx.conn);
    let (a, b) = (player_id(&fx, USR_A), player_id(&fx, USR_B));
    let (v1, v2, v3) = (visit_id(&fx, "2026-06-01"), visit_id(&fx, "2026-06-05"), visit_id(&fx, "2026-06-20"));

    let first = lib.visit_player_badges(v1, 7).unwrap();
    assert_eq!(first.len(), 2);
    assert!(first.iter().all(|p| p.first_met && p.days_since_last.is_none()));

    // A left at 06-01 21:40 and came back 06-05 21:00: 3 whole days
    assert!(lib.visit_player_badges(v2, 7).unwrap().is_empty());
    let r = lib.visit_player_badges(v2, 3).unwrap();
    assert_eq!(r.len(), 1);
    assert_eq!((r[0].player_id, r[0].first_met, r[0].days_since_last), (a, false, Some(3)));
    assert!(lib.visit_player_badges(v2, 4).unwrap().is_empty());

    let r = lib.visit_player_badges(v3, 7).unwrap();
    let get = |id: i64| r.iter().find(|p| p.player_id == id).unwrap().days_since_last;
    assert_eq!((get(a), get(b)), (Some(14), Some(18)));
    // 0 disables the reunion badge, and nobody is a first meeting here
    assert!(lib.visit_player_badges(v3, 0).unwrap().is_empty());
    assert!(lib.visit_player_badges(999_999, 7).unwrap().is_empty());
}

#[test]
fn player_marks_decorate_lists() {
    let fx = setup();
    let lib = LibraryService::new(&fx.conn);
    let (a, b) = (player_id(&fx, USR_A), player_id(&fx, USR_B));
    let t = lib.create_tag("Tagged", "#123456").unwrap();
    lib.set_player_tags(b, &[t.id]).unwrap();
    lib.set_player_memo(a, "memo", dt(2026, 7, 1, 0, 0, 0)).unwrap();

    let marks = lib.player_marks(&[a, b, 123_456]).unwrap();
    assert_eq!(marks.len(), 2);
    let ma = marks.iter().find(|m| m.player_id == a).unwrap();
    let mb = marks.iter().find(|m| m.player_id == b).unwrap();
    assert!(ma.has_memo && ma.tags.is_empty());
    assert!(!mb.has_memo && mb.tags.len() == 1 && mb.tags[0].name == "Tagged");
    assert!(lib.player_marks(&[]).unwrap().is_empty());

    // more ids than one IN chunk
    let many: Vec<i64> = (1..=1200).collect();
    assert_eq!(lib.player_marks(&many).unwrap().len(), 2);
}
