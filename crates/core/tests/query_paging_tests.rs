mod common;

use common::{d, log, Fixture};
use yorimichi_core::data::dtos::RawEventHit;
use yorimichi_core::data::{ImportService, QueryService};

// A: 3 visits, B: visits 1-2, C: visit 3 (after A). Local user is in every visit.
fn sample() -> Fixture {
    let fx = Fixture::new();
    let text = log(r"
        2026.06.01 20:59:00 {P}User Authenticated: Me (usr_aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa)
        2026.06.01 21:00:00 {P}[Behaviour] Joining wrld_bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb:1
        2026.06.01 21:00:05 {P}[Behaviour] OnPlayerJoined Me (usr_aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa)
        2026.06.01 21:00:10 {P}[Behaviour] OnPlayerJoined Alice (usr_11111111-1111-1111-1111-111111111111)
        2026.06.01 21:00:20 {P}[Behaviour] OnPlayerJoined Bob (usr_22222222-2222-2222-2222-222222222222)
        2026.06.01 21:00:20 {W}same second warning one
        2026.06.01 21:00:20 {W}same second warning two
        2026.06.10 21:00:00 {P}[Behaviour] Joining wrld_bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb:2
        2026.06.10 21:00:05 {P}[Behaviour] OnPlayerJoined Me (usr_aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa)
        2026.06.10 21:00:10 {P}[Behaviour] OnPlayerJoined Alice (usr_11111111-1111-1111-1111-111111111111)
        2026.06.10 21:00:20 {P}[Behaviour] OnPlayerJoined Bob (usr_22222222-2222-2222-2222-222222222222)
        2026.06.20 21:00:00 {P}[Behaviour] Joining wrld_bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb:3
        2026.06.20 21:00:05 {P}[Behaviour] OnPlayerJoined Me (usr_aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa)
        2026.06.20 21:00:10 {P}[Behaviour] OnPlayerJoined Alice (usr_11111111-1111-1111-1111-111111111111)
        2026.06.20 21:00:30 {P}[Behaviour] OnPlayerJoined Carol (usr_33333333-3333-3333-3333-333333333333)
    ");
    let path = fx.write_log("output_log_2026-06-01_21-00-00.txt", &text);
    ImportService::new(&fx.conn).import_file(&path, None).unwrap();
    fx
}

#[test]
fn recent_players_exclude_local_and_order_by_last_seen() {
    let fx = sample();
    let q = QueryService::new(&fx.conn);
    let all = q.get_recent_players(None, None, 0, 100).unwrap();
    let names: Vec<&str> = all.iter().map(|p| p.display_name.as_str()).collect();
    assert_eq!(names, vec!["Carol", "Alice", "Bob"]);
    assert!(all.iter().all(|p| !p.is_local));
    assert_eq!(all[1].encounter_count, 3);
    assert!(all[1].last_seen.as_deref().unwrap().starts_with("2026-06-20"));
    assert_eq!(all[1].last_seen_date, "2026-06-20");
}

#[test]
fn recent_players_pages_concatenate() {
    let fx = sample();
    let q = QueryService::new(&fx.conn);
    let all = q.get_recent_players(None, None, 0, 100).unwrap();
    let mut paged = Vec::new();
    let mut off = 0;
    loop {
        let page = q.get_recent_players(None, None, off, 2).unwrap();
        let n = page.len();
        paged.extend(page);
        if n < 2 {
            break;
        }
        off += 2;
    }
    let all_ids: Vec<i64> = all.iter().map(|p| p.id).collect();
    let paged_ids: Vec<i64> = paged.iter().map(|p| p.id).collect();
    assert_eq!(all_ids, paged_ids);
    let mut dedup = paged_ids.clone();
    dedup.sort();
    dedup.dedup();
    assert_eq!(paged_ids.len(), dedup.len());
}

#[test]
fn recent_players_respect_date_filter() {
    let fx = sample();
    let q = QueryService::new(&fx.conn);
    let hits = q.get_recent_players(Some(d(2026, 6, 5)), Some(d(2026, 6, 12)), 0, 100).unwrap();
    let mut names: Vec<&str> = hits.iter().map(|p| p.display_name.as_str()).collect();
    names.sort();
    assert_eq!(names, vec!["Alice", "Bob"]);
    assert!(hits.iter().all(|p| p.encounter_count == 1));
}

#[test]
fn search_players_pages_and_fills_last_seen() {
    let fx = sample();
    let q = QueryService::new(&fx.conn);
    let all = q.search_players("", 0, 200).unwrap();
    assert_eq!(all.len(), 4); // includes local user
    let mut paged = q.search_players("", 0, 3).unwrap();
    paged.extend(q.search_players("", 3, 3).unwrap());
    let all_ids: Vec<i64> = all.iter().map(|p| p.id).collect();
    let paged_ids: Vec<i64> = paged.iter().map(|p| p.id).collect();
    assert_eq!(all_ids, paged_ids);
    assert!(all.iter().all(|p| p.last_seen.as_deref().is_some_and(|s| !s.is_empty())));
    assert_eq!(q.search_players("Car", 0, 10).unwrap().len(), 1);
}

fn page_all(q: &QueryService<'_>, term: &str, level: Option<&str>, size: i64) -> Vec<RawEventHit> {
    let mut result = Vec::new();
    let mut before: Option<(String, i64)> = None;
    loop {
        let page = q.search_raw_events(term, level, None, None, before.clone(), size).unwrap();
        let n = page.len() as i64;
        if let Some(last) = page.last() {
            before = Some((last.event_time.clone(), last.id));
        }
        result.extend(page);
        if n < size {
            break;
        }
    }
    result
}

fn assert_descending(rows: &[RawEventHit]) {
    for w in rows.windows(2) {
        let c = w[0].event_time.cmp(&w[1].event_time);
        assert!(c.is_gt() || (c.is_eq() && w[0].id > w[1].id));
    }
}

fn distinct_ids(rows: &[RawEventHit]) -> usize {
    let mut ids: Vec<i64> = rows.iter().map(|r| r.id).collect();
    ids.sort();
    ids.dedup();
    ids.len()
}

#[test]
fn raw_events_empty_term_pages_every_row_once() {
    let fx = sample();
    let q = QueryService::new(&fx.conn);
    let total = fx.count("SELECT count(*) FROM raw_events");
    let first = q.search_raw_events("", None, None, None, None, 3).unwrap();
    assert_eq!(first.len(), 3);
    let mut stmt = fx
        .conn
        .prepare("SELECT id FROM raw_events ORDER BY event_time DESC, id DESC LIMIT 3")
        .unwrap();
    let newest: Vec<i64> = stmt.query_map([], |r| r.get(0)).unwrap().collect::<Result<_, _>>().unwrap();
    assert_eq!(newest, first.iter().map(|r| r.id).collect::<Vec<_>>());

    let all = page_all(&q, "", None, 3);
    assert_eq!(all.len() as i64, total);
    assert_eq!(all.len(), distinct_ids(&all));
    assert_descending(&all);
}

#[test]
fn raw_events_page_with_term_and_level() {
    let fx = sample();
    let q = QueryService::new(&fx.conn);
    let expected_term = fx.count("SELECT count(*) FROM raw_events WHERE message LIKE '%OnPlayerJoined%'");
    let by_term = page_all(&q, "OnPlayerJoined", None, 2);
    assert_eq!(by_term.len() as i64, expected_term);
    assert_eq!(by_term.len(), distinct_ids(&by_term));
    assert_descending(&by_term);

    let by_level = page_all(&q, "", Some("Warning"), 1);
    assert_eq!(by_level.len(), 2);
    assert!(by_level.iter().all(|r| r.level == "Warning"));
    // same second, two rows
    let mut times: Vec<&str> = by_level.iter().map(|r| r.event_time.as_str()).collect();
    times.sort();
    times.dedup();
    assert_eq!(times.len() + 1, 2);
}
