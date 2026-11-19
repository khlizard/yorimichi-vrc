mod common;

use common::{dt, log};
use vrclogger_core::parsing::{parse, ParseOptions, ParseResult};

fn parse_log(text: &str) -> ParseResult {
    parse(text, &ParseOptions::default())
}

#[test]
fn parses_basic_visit_with_players() {
    let result = parse_log(&log(r"
        2026.06.23 21:38:17 {P}User Authenticated: テスト／Tester (usr_aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa)
        2026.06.23 21:38:23 {P}[Behaviour] Entering Room: Sample World
        2026.06.23 21:38:23 {P}[Behaviour] Joining wrld_bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb:91278~private(usr_aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa)~region(jp)
        2026.06.23 21:38:33 {P}[Behaviour] OnPlayerJoined テスト／Tester (usr_aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa)
        2026.06.23 21:40:46 {P}[Behaviour] OnPlayerLeft テスト／Tester (usr_aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa)
        2026.06.23 21:40:46 {P}[Behaviour] OnLeftRoom
    "));

    assert_eq!(result.local_user_id.as_deref(), Some("usr_aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa"));
    assert_eq!(result.local_user_name.as_deref(), Some("テスト／Tester"));
    assert_eq!(result.visits.len(), 1);
    let visit = &result.visits[0];
    assert_eq!(visit.world_name, "Sample World");
    assert_eq!(visit.world_id.as_deref(), Some("wrld_bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb"));
    assert!(visit.instance_id.starts_with("91278~private"));
    assert!(visit.ended_cleanly);
    assert_eq!(visit.join_time, dt(2026, 6, 23, 21, 38, 23));
    assert_eq!(visit.leave_time, dt(2026, 6, 23, 21, 40, 46));
    assert_eq!(visit.encounters.len(), 1);
    let enc = &visit.encounters[0];
    assert_eq!(enc.display_name, "テスト／Tester");
    assert!(enc.ended_cleanly);
}

#[test]
fn player_left_room_line_is_not_a_player_left() {
    let result = parse_log(&log(r"
        2026.06.23 21:38:23 {P}[Behaviour] Joining wrld_bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb:1
        2026.06.23 21:38:33 {P}[Behaviour] OnPlayerJoined A B (C) (usr_11111111-1111-1111-1111-111111111111)
        2026.06.23 21:49:59 {P}[Behaviour] OnPlayerLeftRoom
        2026.06.23 21:50:00 {P}[Behaviour] OnLeftRoom
    "));

    assert_eq!(result.visits.len(), 1);
    assert_eq!(result.visits[0].encounters.len(), 1);
    let enc = &result.visits[0].encounters[0];
    assert_eq!(enc.display_name, "A B (C)"); // name with spaces and parens, last (usr_...) wins
    assert_eq!(enc.leave_time, dt(2026, 6, 23, 21, 50, 0)); // closed by OnLeftRoom, not OnPlayerLeftRoom
}

#[test]
fn continuation_lines_are_skipped() {
    let result = parse_log(&log(r"
        2026.06.23 21:38:23 {P}[Behaviour] Joining wrld_bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb:1
        2026.06.23 22:08:53 Error      -  HTTP/1.1 404 Not Found
        Cysharp.Threading.Tasks.UnityWebRequestException: HTTP/1.1 404 Not Found
          at VRC.DataModel.Textures.NativeTextureDownloader.DownloadTexture (...)
        2026.06.23 22:09:00 {P}[Behaviour] OnLeftRoom
    "));

    assert_eq!(result.visits.len(), 1);
    // Error line and continuation lines are excluded from raw events.
    assert_eq!(result.raw_events.len(), 2);
    assert!(!result.raw_events.iter().any(|e| e.message.contains("404")));
}

#[test]
fn crash_eof_closes_visit_and_encounters_uncleanly() {
    let result = parse_log(&log(r"
        2026.06.23 21:38:23 {P}[Behaviour] Joining wrld_bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb:1
        2026.06.23 21:38:33 {P}[Behaviour] OnPlayerJoined X (usr_11111111-1111-1111-1111-111111111111)
        2026.06.23 23:00:00 {P}some later line
    "));

    assert_eq!(result.visits.len(), 1);
    let visit = &result.visits[0];
    assert!(!visit.ended_cleanly);
    assert_eq!(visit.leave_time, dt(2026, 6, 23, 23, 0, 0));
    assert_eq!(visit.encounters.len(), 1);
    let enc = &visit.encounters[0];
    assert!(!enc.ended_cleanly);
    assert_eq!(enc.leave_time, dt(2026, 6, 23, 23, 0, 0));
}

#[test]
fn back_to_back_joining_closes_previous_visit() {
    let result = parse_log(&log(r"
        2026.06.23 21:00:00 {P}[Behaviour] Entering Room: First
        2026.06.23 21:00:00 {P}[Behaviour] Joining wrld_11111111-1111-1111-1111-111111111111:1
        2026.06.23 22:00:00 {P}[Behaviour] Entering Room: Second
        2026.06.23 22:00:00 {P}[Behaviour] Joining wrld_22222222-2222-2222-2222-222222222222:2
        2026.06.23 23:00:00 {P}[Behaviour] OnLeftRoom
    "));

    assert_eq!(result.visits.len(), 2);
    assert!(!result.visits[0].ended_cleanly);
    assert_eq!(result.visits[0].leave_time, dt(2026, 6, 23, 22, 0, 0));
    assert_eq!(result.visits[1].world_name, "Second");
    assert!(result.visits[1].ended_cleanly);
}

#[test]
fn url_events_are_deduped_within_window() {
    let result = parse_log(&log(r"
        2026.06.23 21:00:00 {P}[Behaviour] Joining wrld_11111111-1111-1111-1111-111111111111:1
        2026.06.23 21:43:55 {P}[Video Playback] Resolving URL 'rtspt://example.com/live/stream'
        2026.06.23 21:43:57 {P}[Video Playback] Attempting to resolve URL 'rtspt://example.com/live/stream'
        2026.06.23 21:45:37 {P}[Video Playback] Resolving URL 'rtspt://example.com/live/stream'
        2026.06.23 21:46:00 {P}[String Download] Attempting to load String from URL 'https://example.com/data.json'
        2026.06.23 21:46:05 {P}[Image Download] Attempting to load image from URL 'https://example.com/img.png'
        2026.06.23 22:00:00 {P}[Behaviour] OnLeftRoom
    "));

    assert_eq!(result.visits.len(), 1);
    let urls = &result.visits[0].url_events;
    assert_eq!(urls.len(), 4);
    // 21:43:55 kept, :57 deduped, 21:45:37 far enough
    assert_eq!(urls.iter().filter(|u| u.kind == "video").count(), 2);
    assert_eq!(urls.iter().filter(|u| u.kind == "string").count(), 1);
    assert_eq!(urls.iter().filter(|u| u.kind == "image").count(), 1);
}

#[test]
fn raw_events_link_to_current_visit() {
    let result = parse_log(&log(r"
        2026.06.23 20:59:00 {P}before any visit
        2026.06.23 21:00:00 {P}[Behaviour] Joining wrld_11111111-1111-1111-1111-111111111111:1
        2026.06.23 21:30:00 {P}[Behaviour] Pickup object 'Pen'
        2026.06.23 22:00:00 {P}[Behaviour] OnLeftRoom
        2026.06.23 22:01:00 {P}after leaving
    "));

    assert_eq!(result.raw_events[0].visit_index, None);
    assert_eq!(result.raw_events[1].visit_index, Some(0)); // Joining line belongs to the new visit
    assert_eq!(result.raw_events[2].visit_index, Some(0));
    assert_eq!(result.raw_events[3].visit_index, Some(0)); // OnLeftRoom belongs to the visit it ends
    assert_eq!(result.raw_events[4].visit_index, None);
}

#[test]
fn parses_communication_notifications_and_skips_group_bulletins() {
    let result = parse_log(&log(r#"
        2026.06.23 21:38:23 {P}[Behaviour] Joining wrld_11111111-1111-1111-1111-111111111111:1
        2026.06.23 21:39:00 {P}Received Notification: <Notification from username:Friend, A, sender user id:usr_11111111-1111-1111-1111-111111111111 to usr_aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa of type: invite, id: not_11111111-1111-1111-1111-111111111111, created at: 06/23/2026 12:39:00 UTC, details: {{worldId=wrld_22222222-2222-2222-2222-222222222222:1~region(jp), worldName=Some World}}, type:invite, m seen:False, message: "">
        2026.06.23 21:39:00 {P}Received Notification: <Notification from username:Friend B, sender user id:usr_22222222-2222-2222-2222-222222222222 to usr_aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa of type: boop, id: not_22222222-2222-2222-2222-222222222222, created at: 06/23/2026 12:39:00 UTC, details: {{emojiId=default_heart, emojiVersion=, inventoryItemId=}}, type:boop, m seen:False, message: "Friend B Booped You!">
        2026.06.23 21:39:00 {P}Received Notification: <Notification from username:Friend C, sender user id:usr_22222222-2222-2222-2222-222222222222 to usr_aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa of type: friendRequest, id: frq_33333333-3333-3333-3333-333333333333, created at: 06/23/2026 12:39:00 UTC, details: {{}}, type:friendRequest, m seen:False, message: "">
        2026.06.23 21:39:00 {P}Received Notification: <Notification from username:, sender user id: to usr_aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa of type: group, id: not_44444444-4444-4444-4444-444444444444, created at: 06/23/2026 12:39:00 UTC, details: {{}}, type:group, m seen:False, message: "Group bulletin text">
        2026.06.23 21:40:00 {P}AcceptNotification for notification:<Notification from username:Friend, A, sender user id:usr_11111111-1111-1111-1111-111111111111 to usr_aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa of type: invite, id: not_11111111-1111-1111-1111-111111111111, created at: 06/23/2026 12:39:00 UTC, details: {{}}, type:invite, m seen:False, message: "">
        2026.06.23 22:00:00 {P}[Behaviour] OnLeftRoom
    "#));

    assert_eq!(result.notifications.len(), 3);
    assert!(!result.notifications.iter().any(|n| n.kind == "group"));

    let invite = result.notifications.iter().find(|n| n.kind == "invite").unwrap();
    assert_eq!(invite.notification_id, "not_11111111-1111-1111-1111-111111111111");
    assert_eq!(invite.sender_name, "Friend, A"); // comma in the name is not a field separator
    assert_eq!(invite.sender_user_id, "usr_11111111-1111-1111-1111-111111111111");
    assert!(invite.details.contains("worldName=Some World"));
    assert_eq!(invite.message, "");
    assert_eq!(invite.created_at_utc, "06/23/2026 12:39:00");
    assert_eq!(invite.time, dt(2026, 6, 23, 21, 39, 0));

    let boop = result.notifications.iter().find(|n| n.kind == "boop").unwrap();
    assert_eq!(boop.message, "Friend B Booped You!"); // trailing "> stripped

    assert_eq!(
        result.notifications.iter().find(|n| n.kind == "friendRequest").unwrap().notification_id,
        "frq_33333333-3333-3333-3333-333333333333"
    );

    assert_eq!(result.answered_notifications.len(), 1);
    let answered = &result.answered_notifications[0];
    assert_eq!(answered.0, "not_11111111-1111-1111-1111-111111111111");
    assert_eq!(answered.1, dt(2026, 6, 23, 21, 40, 0));

    // Notification lines are still kept in raw events (they are not on the noise list).
    assert_eq!(
        result.raw_events.iter().filter(|r| r.message.starts_with("Received Notification")).count(),
        4
    );
}

#[test]
fn parses_moderation_events() {
    let result = parse_log(&log(r"
        2026.06.23 21:38:23 {P}[Behaviour] Joining wrld_11111111-1111-1111-1111-111111111111:1
        2026.06.23 21:39:00 {P}[ModerationManager] Some One is now Muted
        2026.06.23 21:39:30 {P}[ModerationManager] Some One is no longer Muted
        2026.06.23 21:40:00 {P}[ModerationManager] Bad Actor is now Blocked
        2026.06.23 21:41:00 {P}[ModerationManager] A vote kick has been initiated against Bad Actor, do you agree?
        2026.06.23 21:42:00 {P}[ModerationManager] Some One Avatar will follow Safety Settings
        2026.06.23 22:00:00 {P}[Behaviour] OnLeftRoom
        2026.06.23 22:01:00 {P}[ModerationManager] Outside Visit is no longer Blocked
    "));

    let got: Vec<(&str, &str)> = result
        .moderation_events
        .iter()
        .map(|m| (m.kind.as_str(), m.target_name.as_str()))
        .collect();
    assert_eq!(
        got,
        vec![
            ("mute", "Some One"),
            ("unmute", "Some One"),
            ("block", "Bad Actor"),
            ("votekick", "Bad Actor"),
            ("unblock", "Outside Visit"),
        ]
    );
    assert!(result.moderation_events.iter().take(4).all(|m| m.visit_index == Some(0)));
    assert_eq!(result.moderation_events.last().unwrap().visit_index, None);
    // "Avatar will follow Safety Settings" is noise: neither a moderation event nor a raw event.
    assert!(!result.raw_events.iter().any(|r| r.message.contains("Safety Settings")));
}

#[test]
fn noise_patterns_are_dropped_from_raw_events_but_udon_output_is_kept() {
    let result = parse_log(&log(r"
        2026.06.23 21:38:23 {P}[Behaviour] Joining wrld_11111111-1111-1111-1111-111111111111:1
        2026.06.23 21:38:30 {P}[Behaviour] Using default gesture mask (hands)
        2026.06.23 21:38:31 {P}[Behaviour] CacheComponents: ParticleSystems 0, AudioSources 0
        2026.06.23 21:38:32 {P}Missing button from radio selector group
        2026.06.23 21:38:33 {P}[EOSManager] EOS Initialized Successfully
        2026.06.23 21:38:34 {P}MyWorldScript: player entered trigger zone 3
        2026.06.23 21:38:35 {P}[Udon] custom log line
        2026.06.23 21:38:36 {P}[Behaviour] Switching Some One to avatar Cool Avatar
        2026.06.23 22:00:00 {P}[Behaviour] OnLeftRoom
    "));

    let kept: Vec<&str> = result.raw_events.iter().map(|r| r.message.as_str()).collect();
    assert_eq!(
        kept,
        vec![
            "[Behaviour] Joining wrld_11111111-1111-1111-1111-111111111111:1",
            "MyWorldScript: player entered trigger zone 3",
            "[Udon] custom log line",
            "[Behaviour] Switching Some One to avatar Cool Avatar",
            "[Behaviour] OnLeftRoom",
        ]
    );
}

#[test]
fn custom_noise_patterns_override_defaults() {
    let options = ParseOptions::new(["[Udon]", "*trigger zone"]);
    let result = parse(
        &log(r"
        2026.06.23 21:38:23 {P}[Behaviour] Joining wrld_11111111-1111-1111-1111-111111111111:1
        2026.06.23 21:38:30 {P}[Behaviour] Using default gesture mask (hands)
        2026.06.23 21:38:34 {P}MyWorldScript: player entered trigger zone 3
        2026.06.23 21:38:35 {P}[Udon] custom log line
        2026.06.23 22:00:00 {P}[Behaviour] OnLeftRoom
    "),
        &options,
    );

    let kept: Vec<&str> = result.raw_events.iter().map(|r| r.message.as_str()).collect();
    // default list not applied
    assert!(kept.contains(&"[Behaviour] Using default gesture mask (hands)"));
    assert!(!kept.iter().any(|m| m.starts_with("[Udon]")));
    assert!(!kept.iter().any(|m| m.contains("trigger zone")));
}

#[test]
fn parses_avatar_switches_and_download_sizes() {
    let result = parse_log(&log(r"
        2026.06.23 21:38:23 {P}[Behaviour] Joining wrld_11111111-1111-1111-1111-111111111111:1
        2026.06.23 21:38:30 {P}[Behaviour] Switching Alice to avatar Dress by Shop
        2026.06.23 21:38:30 {P}[Behaviour] Switching Bob to avatar Cached One
        2026.06.23 21:38:30 {P}[AssetBundleDownloadManager] [109] Unpacking Avatar (Cached One by Bob)
        2026.06.23 21:38:31 {P}[AssetBundleDownloadManager] Starting download of Avatar @ 44 MB, 0 still queued.
        2026.06.23 21:38:34 {P}[AssetBundleDownloadManager] Download for avatar (Worn:0 Dist:9.03) (44.2 MB) started 0 seconds and completed 3 seconds after queueing.
        2026.06.23 21:38:34 {P}[AssetBundleDownloadManager] [114] Unpacking Avatar (Dress by Shop by Alice)
        2026.06.23 21:39:00 {P}[Behaviour] Switching Alice to avatar Dress by Shop
        2026.06.23 21:40:00 {P}[AssetBundleDownloadManager] Download for avatar (Worn:0 Dist:2.00) (10.5 MB) started 0 seconds and completed 1 seconds after queueing.
        2026.06.23 21:40:00 {P}[AssetBundleDownloadManager] [115] Unpacking Avatar (Surprise by Carol)
        2026.06.23 21:41:00 {P}[Behaviour] Switching Alice to avatar Second Outfit
        2026.06.23 22:00:00 {P}[Behaviour] OnLeftRoom
    "));

    let events = &result.avatar_events;
    assert_eq!(events.len(), 4);

    let alice = &events[0];
    // split at the last " by "
    assert_eq!((alice.player_name.as_str(), alice.avatar_name.as_str()), ("Alice", "Dress by Shop"));
    assert_eq!(alice.size_mb, Some(44.2));
    assert_eq!(alice.load_seconds, Some(3));
    assert_eq!(alice.visit_index, Some(0));

    let bob = &events[1];
    assert_eq!(bob.avatar_name, "Cached One");
    assert_eq!(bob.size_mb, None); // unpacked from cache: no download line, no size

    // Unpack with a download but no preceding Switching still yields an event.
    let carol = &events[2];
    assert_eq!(
        (carol.player_name.as_str(), carol.avatar_name.as_str(), carol.size_mb),
        ("Carol", "Surprise", Some(10.5))
    );

    // Repeated "Switching" to the same avatar in the same visit is deduped; a new avatar is not.
    assert_eq!(events[3].avatar_name, "Second Outfit");
    assert_eq!(events[3].time, dt(2026, 6, 23, 21, 41, 0));
}
