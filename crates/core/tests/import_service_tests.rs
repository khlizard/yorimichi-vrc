mod common;

use common::{d, dt, log, Fixture, P};
use yorimichi_core::data::{ImportService, QueryService};
use yorimichi_core::parsing::{noise, ParseOptions};

fn sample_log(with_second_player: bool) -> String {
    let mut s = log(r"
        2026.06.23 20:00:17 {P}User Authenticated: テスト／Tester (usr_aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa)
        2026.06.23 20:00:23 {P}[Behaviour] Entering Room: Sample World
        2026.06.23 20:00:23 {P}[Behaviour] Joining wrld_bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb:12345~region(jp)
        2026.06.23 20:00:33 {P}[Behaviour] OnPlayerJoined テスト／Tester (usr_aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa)
        2026.06.23 21:40:00 {P}[Video Playback] Resolving URL 'https://example.com/video.mp4'
    ");
    if with_second_player {
        s.push_str(&format!(
            "2026.06.23 21:41:00 {P}[Behaviour] OnPlayerJoined Friend A (usr_11111111-1111-1111-1111-111111111111)\n"
        ));
    }
    s
}

#[test]
fn imports_and_skips_unchanged() {
    let fx = Fixture::new();
    let path = fx.write_log("output_log_2026-01-10_20-00-00.txt", &sample_log(false));
    let svc = ImportService::new(&fx.conn);

    assert!(svc.import_file(&path, None).unwrap());
    assert!(!svc.import_file(&path, None).unwrap()); // unchanged -> skipped

    assert_eq!(fx.count("SELECT count(*) FROM log_files"), 1);
    assert_eq!(fx.count("SELECT count(*) FROM visits"), 1);
    assert_eq!(fx.count("SELECT count(*) FROM encounters"), 1);
    assert_eq!(fx.count("SELECT count(*) FROM url_events"), 1);
    assert_eq!(fx.count("SELECT count(*) FROM players WHERE is_local = 1"), 1);
    assert_eq!(fx.count("SELECT count(*) FROM raw_events"), 5);
}

#[test]
fn reimports_grown_file_without_duplicates() {
    let fx = Fixture::new();
    let path = fx.write_log("output_log_2026-01-10_20-00-00.txt", &sample_log(false));
    let svc = ImportService::new(&fx.conn);
    svc.import_file(&path, None).unwrap();

    std::fs::write(&path, sample_log(true)).unwrap();
    assert!(svc.import_file(&path, None).unwrap()); // grew -> reimported

    assert_eq!(fx.count("SELECT count(*) FROM log_files"), 1);
    assert_eq!(fx.count("SELECT count(*) FROM visits"), 1);
    assert_eq!(fx.count("SELECT count(*) FROM encounters"), 2);
    assert_eq!(fx.count("SELECT count(*) FROM url_events"), 1);
    assert_eq!(fx.count("SELECT count(*) FROM raw_events"), 6);
    // FTS index followed the delete+reinsert (triggers).
    assert_eq!(fx.count("SELECT count(*) FROM raw_events_fts"), 6);
}

#[test]
fn queries_return_imported_data() {
    let fx = Fixture::new();
    let path = fx.write_log("output_log_2026-01-10_20-00-00.txt", &sample_log(true));
    ImportService::new(&fx.conn).import_file(&path, None).unwrap();
    let q = QueryService::new(&fx.conn);

    let players = q.search_players("Tester", 0, 200).unwrap();
    assert_eq!(players.len(), 1);
    let hit = &players[0];
    assert!(hit.is_local);

    let encounters = q.get_encounters(hit.id, None, None).unwrap();
    assert_eq!(encounters.len(), 1);
    assert_eq!(encounters[0].world_name, "Sample World");

    let visits = q.get_visits(None, None, None).unwrap();
    assert_eq!(visits.len(), 1);
    let visit = &visits[0];
    assert_eq!(visit.player_count, 2);

    assert_eq!(q.get_visit_players(visit.id).unwrap().len(), 2);
    assert_eq!(q.get_visit_urls(visit.id).unwrap().len(), 1);
    assert_eq!(q.search_urls("example.com", None, None, None).unwrap().len(), 1);
    assert!(q.search_urls("example.com", Some("image"), None, None).unwrap().is_empty());

    // FTS search over raw events.
    assert_eq!(q.search_raw_events("Resolving URL", None, None, None, None, 2000).unwrap().len(), 1);
    assert_eq!(q.search_raw_events("OnPlayerJoined", None, None, None, None, 2000).unwrap().len(), 2);

    // Date filter excludes everything outside the range.
    assert!(q.get_visits(None, Some(d(2027, 1, 1)), None).unwrap().is_empty());
    assert_eq!(q.get_visits(None, Some(d(2026, 6, 23)), Some(d(2026, 6, 23))).unwrap().len(), 1);
}

#[test]
fn player_rename_keeps_history() {
    let fx = Fixture::new();
    let svc = ImportService::new(&fx.conn);
    let log1 = log(r"
        2026.06.23 21:00:00 {P}[Behaviour] Joining wrld_11111111-1111-1111-1111-111111111111:1
        2026.06.23 21:00:10 {P}[Behaviour] OnPlayerJoined OldName (usr_11111111-1111-1111-1111-111111111111)
        2026.06.23 22:00:00 {P}[Behaviour] OnLeftRoom
    ");
    let log2 = log(r"
        2026.07.01 21:00:00 {P}[Behaviour] Joining wrld_11111111-1111-1111-1111-111111111111:2
        2026.07.01 21:00:10 {P}[Behaviour] OnPlayerJoined NewName (usr_11111111-1111-1111-1111-111111111111)
        2026.07.01 22:00:00 {P}[Behaviour] OnLeftRoom
    ");
    svc.import_file(&fx.write_log("output_log_2026-06-23_21-00-00.txt", &log1), None).unwrap();
    svc.import_file(&fx.write_log("output_log_2026-07-01_21-00-00.txt", &log2), None).unwrap();

    let q = QueryService::new(&fx.conn);
    let hits = q.search_players("OldName", 0, 200).unwrap(); // old name still searchable
    assert_eq!(hits.len(), 1);
    let hit = &hits[0];
    assert_eq!(hit.display_name, "NewName"); // latest name shown
    assert!(hit.other_names.as_deref().unwrap().contains("OldName"));
    assert_eq!(q.get_encounters(hit.id, None, None).unwrap().len(), 2);
}

fn notification_line(kind: &str, id: &str, sender: &str, sender_id: &str, ts: &str) -> String {
    format!(
        "{ts} {P}Received Notification: <Notification from username:{sender}, sender user id:{sender_id} to usr_aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa of type: {kind}, id: {id}, created at: 06/23/2026 12:39:00 UTC, details: {{{{}}}}, type:{kind}, m seen:False, message: \"\">"
    )
}

#[test]
fn notifications_are_deduped_across_files_and_survive_reimport() {
    let fx = Fixture::new();
    let svc = ImportService::new(&fx.conn);
    let log1 = format!(
        "{}\n{}\n2026.06.23 21:46:00 {P}[ModerationManager] Some One is now Muted\n",
        sample_log(false),
        notification_line(
            "boop",
            "not_11111111-1111-1111-1111-111111111111",
            "Friend A",
            "usr_11111111-1111-1111-1111-111111111111",
            "2026.06.23 21:45:00"
        ),
    );
    let accept = format!(
        "2026.06.24 21:01:00 {P}AcceptNotification for notification:<Notification from username:Friend B, sender user id:usr_22222222-2222-2222-2222-222222222222 to usr_aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa of type: friendRequest, id: frq_22222222-2222-2222-2222-222222222222, created at: 06/23/2026 12:39:00 UTC, details: {{{{}}}}, type:friendRequest, m seen:False, message: \"\">"
    );
    // Second session re-logs the same notification (VRChat replays recent notifications on login).
    let log2 = format!(
        "2026.06.24 21:00:00 {P}[Behaviour] Joining wrld_11111111-1111-1111-1111-111111111111:2\n{}\n{}\n{}\n2026.06.24 22:00:00 {P}[Behaviour] OnLeftRoom\n",
        notification_line(
            "boop",
            "not_11111111-1111-1111-1111-111111111111",
            "Friend A",
            "usr_11111111-1111-1111-1111-111111111111",
            "2026.06.24 21:00:10"
        ),
        notification_line(
            "friendRequest",
            "frq_22222222-2222-2222-2222-222222222222",
            "Friend B",
            "usr_22222222-2222-2222-2222-222222222222",
            "2026.06.24 21:00:20"
        ),
        accept,
    );
    let path1 = fx.write_log("output_log_2026-01-10_20-00-00.txt", &log1);
    svc.import_file(&path1, None).unwrap();
    svc.import_file(&fx.write_log("output_log_2026-06-24_21-00-00.txt", &log2), None).unwrap();

    assert_eq!(fx.count("SELECT count(*) FROM notifications"), 2);
    // First sighting wins for event_time.
    let s = |sql: &str| -> String { fx.conn.query_row(sql, [], |r| r.get(0)).unwrap() };
    assert_eq!(
        s("SELECT event_time FROM notifications WHERE notification_id = 'not_11111111-1111-1111-1111-111111111111'"),
        "2026-06-23 21:45:00"
    );
    assert_eq!(
        s("SELECT accepted_at FROM notifications WHERE notification_id = 'frq_22222222-2222-2222-2222-222222222222'"),
        "2026-06-24 21:01:00"
    );
    assert_eq!(fx.count("SELECT count(*) FROM moderation_events"), 1);

    // File 1 grows and is reimported: its cascade delete must not take the shared notification with it.
    std::fs::write(&path1, format!("{log1}2026.06.23 21:50:00 {P}[Behaviour] OnLeftRoom\n")).unwrap();
    assert!(svc.import_file(&path1, None).unwrap());
    assert_eq!(fx.count("SELECT count(*) FROM notifications"), 2);
    // replaced, not duplicated
    assert_eq!(fx.count("SELECT count(*) FROM moderation_events"), 1);

    let q = QueryService::new(&fx.conn);
    assert_eq!(q.search_notifications("", None, None, None).unwrap().len(), 2);
    assert_eq!(q.search_notifications("Friend B", None, None, None).unwrap().len(), 1);
    assert_eq!(q.search_notifications("", Some("boop"), None, None).unwrap().len(), 1);
    assert_eq!(q.search_notifications("", Some("friendRequest"), None, None).unwrap()[0].response, "応答済");
    assert_eq!(q.get_notification_kinds().unwrap(), vec!["boop", "friendRequest"]);

    let mods = q.search_moderation_events("Some", None, None).unwrap();
    assert_eq!(mods.len(), 1);
    assert_eq!(mods[0].kind, "mute");
    assert_eq!(mods[0].world_name, "Sample World");
    assert!(mods[0].visit_id.is_some());
}

#[test]
fn prune_deletes_expired_and_noise_raw_events_only() {
    let fx = Fixture::new();
    // import everything so Prune has noise to clean
    let svc = ImportService::with_options(&fx.conn, ParseOptions::new(Vec::<String>::new()));
    let text = log(r"
        2026.06.01 21:00:00 {P}[Behaviour] Joining wrld_11111111-1111-1111-1111-111111111111:1
        2026.06.01 21:00:10 {P}[Behaviour] OnPlayerJoined Old Friend (usr_11111111-1111-1111-1111-111111111111)
        2026.06.01 21:00:20 {P}[Video Playback] Resolving URL 'https://example.com/old.mp4'
        2026.06.01 22:00:00 {P}[Behaviour] OnLeftRoom
        2026.07.01 21:00:00 {P}[Behaviour] Joining wrld_11111111-1111-1111-1111-111111111111:2
        2026.07.01 21:00:10 {P}[Behaviour] Using default gesture mask (hands)
        2026.07.01 21:00:11 {P}Some One Avatar will follow Safety Settings
        2026.07.01 21:00:12 {P}MyWorldScript: 100% done
        2026.07.01 22:00:00 {P}[Behaviour] OnLeftRoom
    ");
    svc.import_file(&fx.write_log("output_log_2026-06-01_21-00-00.txt", &text), None).unwrap();
    assert_eq!(fx.count("SELECT count(*) FROM raw_events"), 9);

    let pruned = svc.prune(30, noise::DEFAULT, Some(dt(2026, 7, 10, 0, 0, 0))).unwrap();

    assert_eq!(pruned.expired_deleted, 4); // everything from June
    assert_eq!(pruned.noise_deleted, 2); // gesture mask + Safety Settings (contains-match)
    let mut stmt = fx.conn.prepare("SELECT message FROM raw_events ORDER BY event_time").unwrap();
    let remaining: Vec<String> =
        stmt.query_map([], |r| r.get(0)).unwrap().collect::<Result<_, _>>().unwrap();
    assert_eq!(
        remaining,
        vec![
            "[Behaviour] Joining wrld_11111111-1111-1111-1111-111111111111:2",
            "MyWorldScript: 100% done",
            "[Behaviour] OnLeftRoom",
        ]
    );
    // triggers kept FTS in sync
    assert_eq!(fx.count("SELECT count(*) FROM raw_events_fts"), 3);

    // Structured data is untouched.
    assert_eq!(fx.count("SELECT count(*) FROM visits"), 2);
    assert_eq!(fx.count("SELECT count(*) FROM encounters"), 1);
    assert_eq!(fx.count("SELECT count(*) FROM url_events"), 1);

    // Retention 0 keeps everything; noise list still applies (nothing left to remove).
    assert_eq!(svc.prune(0, noise::DEFAULT, Some(dt(2030, 1, 1, 0, 0, 0))).unwrap().total(), 0);
}

#[test]
fn avatar_events_are_linked_to_players_and_queryable() {
    let fx = Fixture::new();
    let svc = ImportService::new(&fx.conn);
    let text = log(r"
        2026.06.23 20:00:23 {P}[Behaviour] Entering Room: Sample World
        2026.06.23 20:00:23 {P}[Behaviour] Joining wrld_11111111-1111-1111-1111-111111111111:1
        2026.06.23 20:00:25 {P}[Behaviour] OnPlayerJoined Alice (usr_11111111-1111-1111-1111-111111111111)
        2026.06.23 20:00:30 {P}[Behaviour] Switching Alice to avatar First
        2026.06.23 20:00:34 {P}[AssetBundleDownloadManager] Download for avatar (Worn:0 Dist:9.03) (44.2 MB) started 0 seconds and completed 3 seconds after queueing.
        2026.06.23 20:00:34 {P}[AssetBundleDownloadManager] [114] Unpacking Avatar (First by Alice)
        2026.06.23 21:50:00 {P}[Behaviour] Switching Alice to avatar Second
        2026.06.23 22:00:00 {P}[Behaviour] OnLeftRoom
    ");
    svc.import_file(&fx.write_log("output_log_2026-01-10_20-00-00.txt", &text), None).unwrap();

    assert_eq!(fx.count("SELECT count(*) FROM avatar_events"), 2);
    assert_eq!(fx.count("SELECT count(*) FROM avatar_events WHERE player_id IS NOT NULL"), 2);

    let q = QueryService::new(&fx.conn);
    let visits = q.get_visits(None, None, None).unwrap();
    assert_eq!(visits.len(), 1);
    let visit = &visits[0];
    let players = q.get_visit_players(visit.id).unwrap();
    assert_eq!(players.len(), 1);
    let player = &players[0];
    assert_eq!(player.avatar_name.as_deref(), Some("Second")); // latest avatar in the visit
    assert_eq!(player.avatar_size_mb, Some(44.2)); // last known size (from the earlier download)

    let hits = q.search_players("Alice", 0, 200).unwrap();
    assert_eq!(hits.len(), 1);
    let encs = q.get_encounters(hits[0].id, None, None).unwrap();
    assert_eq!(encs.len(), 1);
    assert_eq!(encs[0].avatar_name.as_deref(), Some("Second"));

    let rows = q.search_avatars("First", None, None).unwrap();
    assert_eq!(rows.len(), 1);
    let row = &rows[0];
    assert_eq!(row.display_name, "Alice");
    assert_eq!(row.size_mb, Some(44.2));
    assert_eq!(row.load_seconds, Some(3));
    assert_eq!(row.world_name, "Sample World");
    assert_eq!(row.visit_id, Some(visit.id));
    // player name matches too (LIKE is case-insensitive for ASCII)
    assert_eq!(q.search_avatars("alice", None, None).unwrap().len(), 2);
    assert!(q.search_avatars("First", Some(d(2027, 1, 1)), None).unwrap().is_empty());
}

#[test]
fn backfill_recovers_avatar_switches_from_raw_events() {
    let fx = Fixture::new();
    // keep every line in raw_events
    let svc = ImportService::with_options(&fx.conn, ParseOptions::new(Vec::<String>::new()));
    let text = log(r"
        2026.06.23 20:00:23 {P}[Behaviour] Joining wrld_11111111-1111-1111-1111-111111111111:1
        2026.06.23 20:00:25 {P}[Behaviour] OnPlayerJoined Alice (usr_11111111-1111-1111-1111-111111111111)
        2026.06.23 20:00:30 {P}[Behaviour] Switching Alice to avatar Dress by Shop
        2026.06.23 20:00:31 {P}[Behaviour] Switching Alice to avatar Dress by Shop
        2026.06.23 21:50:00 {P}[Behaviour] Switching Alice to avatar Second
        2026.06.23 22:00:00 {P}[Behaviour] OnLeftRoom
        2026.06.23 22:01:00 {P}[Behaviour] Switching Nobody to avatar Lobby Look
    ");
    svc.import_file(&fx.write_log("output_log_2026-01-10_20-00-00.txt", &text), None).unwrap();
    assert_eq!(fx.count("SELECT count(*) FROM avatar_events"), 3);

    // Simulate a database that predates avatar tracking.
    fx.conn.execute("DELETE FROM avatar_events", []).unwrap();
    assert_eq!(svc.backfill_avatar_events_from_raw_events().unwrap(), 3);
    assert_eq!(svc.backfill_avatar_events_from_raw_events().unwrap(), 0); // idempotent

    let mut stmt = fx
        .conn
        .prepare("SELECT display_name, avatar_name, visit_id, player_id FROM avatar_events ORDER BY event_time")
        .unwrap();
    let rows: Vec<(String, String, Option<i64>, Option<i64>)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    let avatars: Vec<&str> = rows.iter().map(|r| r.1.as_str()).collect();
    assert_eq!(avatars, vec!["Dress by Shop", "Second", "Lobby Look"]);
    for r in rows.iter().take(2) {
        assert!(r.2.is_some());
        assert!(r.3.is_some());
    }
    assert!(rows[2].2.is_none());
}
