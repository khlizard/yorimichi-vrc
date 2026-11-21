mod common;

use common::{dt, log, Fixture};
use yorimichi_core::data::ImportService;
use yorimichi_core::export::{self, PLACEHOLDER};

const EVIL_NAME: &str = "</script><b>x";

fn sample() -> Fixture {
    let fx = Fixture::new();
    let text = log(&format!(
        r#"
        2026.06.23 20:00:17 {{P}}User Authenticated: Tester (usr_aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa)
        2026.06.23 20:00:23 {{P}}[Behaviour] Entering Room: World One
        2026.06.23 20:00:23 {{P}}[Behaviour] Joining wrld_bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb:1~region(jp)
        2026.06.23 20:00:33 {{P}}[Behaviour] OnPlayerJoined Tester (usr_aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa)
        2026.06.23 21:39:00 {{P}}[Behaviour] OnPlayerJoined {EVIL_NAME} (usr_11111111-1111-1111-1111-111111111111)
        2026.06.23 21:45:00 {{P}}Received Notification: <Notification from username:Friend A, sender user id:usr_11111111-1111-1111-1111-111111111111 to usr_aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa of type: boop, id: not_11111111-1111-1111-1111-111111111111, created at: 06/23/2026 12:39:00 UTC, details: {{{{}}}}, type:boop, m seen:False, message: "">
        2026.06.23 22:00:00 {{P}}[Behaviour] OnLeftRoom
        2026.06.23 22:10:00 {{P}}[Behaviour] Entering Room: World Two
        2026.06.23 22:10:00 {{P}}[Behaviour] Joining wrld_cccccccc-cccc-cccc-cccc-cccccccccccc:2
        2026.06.23 22:10:10 {{P}}[Behaviour] OnPlayerJoined テスト太郎 (usr_22222222-2222-2222-2222-222222222222)
        2026.06.23 22:30:00 {{P}}[Behaviour] OnLeftRoom
        "#
    ));
    let path = fx.write_log("output_log_2026-01-10_20-00-00.txt", &text);
    ImportService::new(&fx.conn).import_file(&path, None).unwrap();
    fx
}

#[test]
fn exports_html_with_embedded_json() {
    let fx = sample();
    let now = dt(2026, 6, 24, 1, 2, 3);
    let output = fx.dir.path().join("out.html");

    export::export(&fx.conn, &output, now).unwrap();

    assert!(output.exists());
    assert!(!fx.dir.path().join("out.html.tmp").exists());
    let html = std::fs::read_to_string(&output).unwrap();
    assert!(!html.contains(PLACEHOLDER));

    let json = export::build_json(&fx.conn, now).unwrap();
    assert!(html.contains(&json));
    assert!(!html.contains(EVIL_NAME));

    let root: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(root["generatedAt"], "2026-06-24 01:02:03");
    let len = |k: &str| root[k].as_array().unwrap().len() as i64;
    assert_eq!(fx.count("SELECT count(*) FROM players"), len("players"));
    assert_eq!(fx.count("SELECT count(*) FROM visits"), len("visits"));
    assert_eq!(fx.count("SELECT count(*) FROM encounters"), len("enc"));
    assert_eq!(fx.count("SELECT count(*) FROM notifications"), len("notif"));
    assert!(len("notif") >= 1);
    assert_eq!(len("visits"), 2);

    let players = root["players"].as_array().unwrap();
    let local: Vec<_> = players
        .iter()
        .filter(|p| p[1] == "usr_aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa")
        .collect();
    assert_eq!(local.len(), 1);
    assert_eq!(local[0][3], 1);
    assert!(players.iter().any(|p| p[2] == EVIL_NAME));
    assert!(players.iter().any(|p| p[2] == "テスト太郎"));
    assert!(json.contains("テスト太郎")); // Japanese stays unescaped
}

#[test]
fn second_export_overwrites() {
    let fx = sample();
    let output = fx.dir.path().join("out.html");
    let now1 = dt(2026, 6, 24, 1, 2, 3);
    let now2 = dt(2026, 6, 25, 4, 5, 6);

    export::export(&fx.conn, &output, now1).unwrap();
    export::export(&fx.conn, &output, now2).unwrap();

    let html = std::fs::read_to_string(&output).unwrap();
    assert!(html.contains("2026-06-25 04:05:06"));
    assert!(!html.contains("2026-06-24 01:02:03"));
    assert!(!fx.dir.path().join("out.html.tmp").exists());
}

#[test]
fn creates_missing_directory() {
    let fx = sample();
    let output = fx.dir.path().join("sub").join("deeper").join("out.html");

    export::export(&fx.conn, &output, dt(2026, 6, 24, 1, 2, 3)).unwrap();

    assert!(output.exists());
    assert!(!output.with_file_name("out.html.tmp").exists());
}
