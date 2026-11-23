mod common;

use common::*;
use yorimichi_core::data::LibraryService;

const W1: &str = "wrld_aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa";
const W2: &str = "wrld_bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb";
const W3: &str = "wrld_cccccccc-cccc-cccc-cccc-cccccccccccc";
const W4: &str = "wrld_dddddddd-dddd-dddd-dddd-dddddddddddd";
const H: u32 = 6;

fn setup() -> Fixture {
    let fx = Fixture::new();
    import_visits(
        &fx,
        &[
            // logical day 2026-11-19 (with day_start_hour = 6)
            V::new("2026.11.19", "21:00:00", "22:00:00", "World One", W1).with("Friend A", USR_A, "21:10:00", "21:40:00"),
            V::new("2026.11.20", "02:00:00", "03:00:00", "World Two", W2).with("Friend B", USR_B, "02:10:00", "02:50:00"),
            // logical day 2026-11-20
            V::new("2026.11.20", "07:00:00", "08:00:00", "World One", W1).with("Friend A", USR_A, "07:10:00", "07:40:00"),
            // earlier years
            V::new("2025.11.19", "20:00:00", "21:00:00", "Old World", W3).with("Friend A", USR_A, "20:10:00", "20:40:00"),
            V::new("2024.02.29", "12:00:00", "13:00:00", "Leap World", W4),
        ],
    );
    add_photo(&fx, "p1.png", "2026-11-19 21:15:00"); // visit 1
    add_photo(&fx, "p2.png", "2026-11-20 02:30:00"); // visit 2 (still the 19th)
    add_photo(&fx, "p3.png", "2026-11-19 23:30:00"); // loose
    add_photo(&fx, "p4.png", "2026-11-20 05:59:59"); // loose, last second of the 19th
    add_photo(&fx, "p5.png", "2026-11-20 06:00:00"); // loose, first second of the 20th
    add_photo(&fx, "p6.png", "2026-11-20 07:30:00"); // visit 3
    add_photo(&fx, "p7.png", "2024-11-19 12:00:00"); // loose, older year
    fx
}

#[test]
fn logical_day_groups_visits_and_photos() {
    let fx = setup();
    let lib = LibraryService::new(&fx.conn);

    let d19 = lib.timeline_day(d(2026, 11, 19), H, 7, true).unwrap();
    assert_eq!(d19.day, "2026-11-19");
    assert_eq!((d19.start.as_str(), d19.end.as_str()), ("2026-11-19 06:00:00", "2026-11-20 06:00:00"));
    assert_eq!(d19.visit_count, 2);
    assert_eq!(d19.total_seconds, 7200);
    assert_eq!(d19.people_count, 2);
    assert_eq!(d19.photo_count, 4);
    let worlds: Vec<_> = d19.visits.iter().map(|v| v.world_name.as_str()).collect();
    assert_eq!(worlds, ["World One", "World Two"]); // the 02:00 visit on the 20th is part of the 19th
    assert_eq!(d19.visits[0].photos.len(), 1);
    assert_eq!(d19.visits[1].photos.len(), 1);
    let loose: Vec<_> = d19.loose_photos.iter().map(|p| p.taken_at.as_str()).collect();
    assert_eq!(loose, ["2026-11-19 23:30:00", "2026-11-20 05:59:59"]);
    assert_eq!(d19.visits[0].world_id.as_deref(), Some(W1));
    assert_eq!(d19.visits[0].seconds, 3600);
    assert_eq!(d19.visits[0].duration, "1h 00m");

    let d20 = lib.timeline_day(d(2026, 11, 20), H, 7, true).unwrap();
    assert_eq!((d20.visit_count, d20.photo_count, d20.people_count, d20.total_seconds), (1, 2, 1, 3600));
    assert_eq!(d20.loose_photos.len(), 1);
    assert_eq!(d20.loose_photos[0].taken_at, "2026-11-20 06:00:00");

    // with a midnight start the 02:00 visit belongs to the 20th
    let m20 = lib.timeline_day(d(2026, 11, 20), 0, 7, true).unwrap();
    assert_eq!(m20.visit_count, 2);

    let empty = lib.timeline_day(d(2020, 1, 1), H, 7, true).unwrap();
    assert_eq!((empty.visit_count, empty.photo_count, empty.visits.len(), empty.loose_photos.len()), (0, 0, 0, 0));
    assert_eq!(empty.day, "2020-01-01");
}

#[test]
fn timeline_players_carry_badges_and_exclude_local() {
    let fx = setup();
    let lib = LibraryService::new(&fx.conn);
    let a = player_id(&fx, USR_A);
    lib.set_player_memo(a, "note", dt(2026, 11, 20, 0, 0, 0)).unwrap();

    let day = lib.timeline_day(d(2026, 11, 19), H, 30, true).unwrap();
    let pa = &day.visits[0].players;
    assert_eq!(pa.len(), 1); // the local player is not listed
    assert_eq!(pa[0].user_id, USR_A);
    assert!(pa[0].has_memo);
    assert!(!pa[0].first_met); // first met in 2025
    assert_eq!(pa[0].days_since_last, Some(365)); // 2025-11-19 20:40 -> 2026-11-19 21:00
    let pb = &day.visits[1].players;
    assert!(pb[0].first_met);
    assert!(!pb[0].has_memo);

    let quiet = lib.timeline_day(d(2026, 11, 19), H, 0, false).unwrap();
    assert!(quiet.visits.iter().flat_map(|v| &v.players).all(|p| !p.first_met && p.days_since_last.is_none()));
}

#[test]
fn activity_and_adjacent_days() {
    let fx = setup();
    let lib = LibraryService::new(&fx.conn);
    let act = lib.activity(d(2026, 11, 1), d(2026, 11, 30), H).unwrap();
    assert_eq!(act.iter().map(|a| a.day.as_str()).collect::<Vec<_>>(), ["2026-11-19", "2026-11-20"]);
    let a19 = &act[0];
    assert_eq!((a19.seconds, a19.visit_count, a19.people_count, a19.photo_count), (7200, 2, 2, 4));
    let a20 = &act[1];
    assert_eq!((a20.seconds, a20.visit_count, a20.people_count, a20.photo_count), (3600, 1, 1, 2));

    // a day with only photos appears too
    let all = lib.activity(d(2024, 1, 1), d(2026, 12, 31), H).unwrap();
    let photo_only = all.iter().find(|a| a.day == "2024-11-19").unwrap();
    assert_eq!((photo_only.visit_count, photo_only.photo_count, photo_only.seconds), (0, 1, 0));
    assert_eq!(all.len(), 5);

    assert_eq!(lib.adjacent_active_day(d(2026, 11, 20), H, false).unwrap().as_deref(), Some("2026-11-19"));
    assert_eq!(lib.adjacent_active_day(d(2026, 11, 19), H, true).unwrap().as_deref(), Some("2026-11-20"));
    assert_eq!(lib.adjacent_active_day(d(2026, 11, 20), H, true).unwrap(), None);
    assert_eq!(lib.adjacent_active_day(d(2026, 11, 19), H, false).unwrap().as_deref(), Some("2025-11-19"));
    assert_eq!(lib.adjacent_active_day(d(2025, 11, 19), H, false).unwrap().as_deref(), Some("2024-11-19"));
    assert_eq!(lib.adjacent_active_day(d(2000, 1, 1), H, false).unwrap(), None);
    assert_eq!(lib.latest_active_day(H).unwrap().as_deref(), Some("2026-11-20"));
}

#[test]
fn empty_database_has_no_active_days() {
    let fx = Fixture::new();
    let lib = LibraryService::new(&fx.conn);
    assert_eq!(lib.latest_active_day(H).unwrap(), None);
    assert!(lib.years().unwrap().is_empty());
    assert!(lib.on_this_day(d(2026, 11, 19), H).unwrap().is_empty());
    assert!(lib.activity(d(2026, 1, 1), d(2026, 12, 31), H).unwrap().is_empty());
}

#[test]
fn visit_seconds_are_capped_at_24_hours() {
    let fx = Fixture::new();
    let mut v = V::new("2026.12.01", "10:00:00", "16:00:00", "Long World", W1);
    v.end_date = "2026.12.02";
    import_visits(&fx, &[v]);
    let lib = LibraryService::new(&fx.conn);
    let act = lib.activity(d(2026, 12, 1), d(2026, 12, 2), H).unwrap();
    // Capped at 10:00 on the 2nd, then split at the 06:00 day start.
    let days: Vec<_> = act.iter().map(|a| (a.day.as_str(), a.seconds, a.visit_count)).collect();
    assert_eq!(days, [("2026-12-01", 20 * 3600, 1), ("2026-12-02", 4 * 3600, 1)]);
}

#[test]
fn visit_running_past_the_day_start_also_opens_the_next_day() {
    let fx = Fixture::new();
    import_visits(
        &fx,
        &[
            // 04:00-09:00: 2 h on the 20th (logical), 3 h on the 21st
            V::new("2026.11.21", "04:00:00", "09:00:00", "Night World", W2).with("Friend B", USR_B, "04:10:00", "08:50:00"),
            V::new("2026.11.21", "10:00:00", "11:00:00", "Morning World", W1),
        ],
    );
    add_photo(&fx, "night.png", "2026-11-21 05:00:00");
    add_photo(&fx, "morning.png", "2026-11-21 08:00:00");
    let lib = LibraryService::new(&fx.conn);

    let d20 = lib.timeline_day(d(2026, 11, 20), H, 0, false).unwrap();
    assert_eq!(d20.visits.len(), 1);
    assert!(!d20.visits[0].carried_over);
    assert_eq!((d20.visit_count, d20.total_seconds, d20.people_count, d20.photo_count), (1, 2 * 3600, 1, 1));
    assert_eq!(d20.visits[0].photos.len(), 1);

    let d21 = lib.timeline_day(d(2026, 11, 21), H, 0, false).unwrap();
    let worlds: Vec<_> = d21.visits.iter().map(|v| (v.world_name.as_str(), v.carried_over)).collect();
    assert_eq!(worlds, [("Night World", true), ("Morning World", false)]);
    // Only the part after 06:00 counts toward the day; the visit itself keeps its full length.
    assert_eq!((d21.visit_count, d21.total_seconds, d21.people_count, d21.photo_count), (2, 4 * 3600, 1, 1));
    assert_eq!((d21.visits[0].seconds, d21.visits[0].duration.as_str()), (5 * 3600, "5h 00m"));
    let night: Vec<_> = d21.visits[0].photos.iter().map(|p| p.taken_at.as_str()).collect();
    assert_eq!(night, ["2026-11-21 08:00:00"]);
    assert!(d21.loose_photos.is_empty(), "the 08:00 photo is not in a time without a log");

    let act = lib.activity(d(2026, 11, 20), d(2026, 11, 21), H).unwrap();
    let days: Vec<_> = act.iter().map(|a| (a.day.as_str(), a.seconds, a.visit_count, a.people_count, a.photo_count)).collect();
    assert_eq!(days, [("2026-11-20", 2 * 3600, 1, 1, 1), ("2026-11-21", 4 * 3600, 2, 1, 1)]);
    // A range that starts on the 21st still sees the visit joined the day before.
    let only21 = lib.activity(d(2026, 11, 21), d(2026, 11, 21), H).unwrap();
    assert_eq!((only21[0].seconds, only21[0].visit_count), (4 * 3600, 2));

    let memories = lib.on_this_day(d(2027, 11, 21), H).unwrap();
    assert_eq!(memories[0].worlds, ["Night World", "Morning World"]);
    assert_eq!((memories[0].visit_count, memories[0].people_count), (2, 1));

    // With a midnight start nothing crosses a day start.
    let m21 = lib.timeline_day(d(2026, 11, 21), 0, 0, false).unwrap();
    assert!(m21.visits.iter().all(|v| !v.carried_over));
    assert_eq!((m21.visit_count, m21.total_seconds), (2, 6 * 3600));
}

#[test]
fn on_this_day_across_years_and_leap_day() {
    let fx = setup();
    let lib = LibraryService::new(&fx.conn);
    let fav = add_photo(&fx, "fav.png", "2025-11-19 20:30:00");
    fx.conn.execute("UPDATE photos SET favorite = 1 WHERE id = ?1", [fav]).unwrap();
    for i in 0..12 {
        add_photo(&fx, &format!("many{i}.png"), &format!("2025-11-19 20:{:02}:00", 11 + i));
    }

    let memories = lib.on_this_day(d(2026, 11, 19), H).unwrap();
    assert_eq!(memories.iter().map(|m| m.year).collect::<Vec<_>>(), [2025, 2024]); // newest first
    let m25 = &memories[0];
    assert_eq!(m25.day, "2025-11-19");
    assert_eq!((m25.visit_count, m25.people_count, m25.photo_count), (1, 1, 13));
    assert_eq!(m25.worlds, ["Old World"]);
    assert_eq!(m25.photos.len(), 8);
    assert!(m25.photos[0].favorite, "favorites come first");
    let m24 = &memories[1]; // photo only
    assert_eq!((m24.visit_count, m24.photo_count, m24.worlds.len()), (0, 1, 0));

    // Feb 29 only exists in leap years
    let leap = lib.on_this_day(d(2028, 2, 29), H).unwrap();
    assert_eq!(leap.iter().map(|m| m.day.as_str()).collect::<Vec<_>>(), ["2024-02-29"]);
    assert_eq!(leap[0].worlds, ["Leap World"]);
    // a later year has nothing to remember
    assert!(lib.on_this_day(d(2024, 11, 19), H).unwrap().is_empty());
}

#[test]
fn stats_totals_and_top_lists_exclude_local() {
    let fx = setup();
    let lib = LibraryService::new(&fx.conn);
    let local = player_id(&fx, LOCAL_USR);
    let (a, b) = (player_id(&fx, USR_A), player_id(&fx, USR_B));

    let s = lib.stats(None, None, H, 10).unwrap();
    assert_eq!(s.total_seconds, 5 * 3600);
    assert_eq!((s.visit_count, s.world_count, s.people_count, s.photo_count, s.active_days), (5, 4, 2, 7, 5));
    assert_eq!(s.top_players.iter().map(|p| p.player_id).collect::<Vec<_>>(), [a, b]);
    assert!(s.top_players.iter().all(|p| p.player_id != local));
    assert_eq!((s.top_players[0].seconds, s.top_players[0].count), (3 * 1800, 3));
    assert_eq!((s.top_players[1].seconds, s.top_players[1].count), (2400, 1));
    assert_eq!(s.top_players[0].user_id, USR_A);
    assert_eq!(s.top_worlds[0].name, "World One");
    assert_eq!((s.top_worlds[0].seconds, s.top_worlds[0].count), (7200, 2));
    assert_eq!(s.top_worlds[0].world_ref_id, Some(world_ref(&fx, W1)));
    assert_eq!(s.top_worlds.len(), 4);

    let limited = lib.stats(None, None, H, 1).unwrap();
    assert_eq!((limited.top_players.len(), limited.top_worlds.len()), (1, 1));

    let one_day = lib.stats(Some(d(2026, 11, 20)), Some(d(2026, 11, 20)), H, 10).unwrap();
    assert_eq!((one_day.visit_count, one_day.photo_count, one_day.active_days, one_day.people_count), (1, 2, 1, 1));
    assert_eq!(one_day.top_players.len(), 1);

    let none = lib.stats(Some(d(2030, 1, 1)), None, H, 10).unwrap();
    assert_eq!((none.total_seconds, none.visit_count, none.active_days), (0, 0, 0));
    assert!(none.top_players.is_empty() && none.top_worlds.is_empty());
}

#[test]
fn years_lists_visit_and_photo_years() {
    let fx = setup();
    assert_eq!(LibraryService::new(&fx.conn).years().unwrap(), [2026, 2025, 2024]);
}
