mod common;

use common::*;
use vrclogger_core::data::LibraryService;
use vrclogger_core::{Error, LibraryError};
use vrclogger_core::export::story::{export_album, export_day, StoryOptions};

const W1: &str = "wrld_aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa";
const EVIL_WORLD: &str = "<script>alert(1)</script> World";
const FRIEND: &str = "Friend & Co";

struct Setup {
    fx: Fixture,
    album: i64,
}

fn setup() -> Setup {
    let fx = Fixture::new();
    import_visits(
        &fx,
        &[V::new("2026.11.19", "21:00:00", "22:00:00", EVIL_WORLD, W1).with(FRIEND, USR_A, "21:10:00", "21:40:00")],
    );
    let ok1 = add_real_photo(&fx, "ok1.png", "2026-11-19 21:15:00");
    let ok2 = add_real_photo(&fx, "ok2.png", "2026-11-19 23:30:00"); // loose
    let nofile = add_photo(&fx, "nofile.png", "2026-11-19 21:20:00"); // row without a file
    let gone = add_real_photo(&fx, "gone.png", "2026-11-19 21:25:00");
    fx.conn.execute("UPDATE photos SET missing = 1 WHERE id = ?1", [gone]).unwrap();
    let lib = LibraryService::new(&fx.conn);
    let album = lib.create_album("Evening <b>trip</b>", "Line one\nsecond & last", dt(2026, 11, 20, 0, 0, 0)).unwrap();
    lib.add_photos(album.id, &[ok1, nofile, gone, ok2]).unwrap();
    Setup { fx, album: album.id }
}

fn now() -> chrono::NaiveDateTime {
    dt(2026, 11, 20, 1, 2, 3)
}

fn assert_private(html: &str, fx: &Fixture) {
    assert!(!html.contains("usr_"));
    assert!(!html.contains("wrld_"));
    assert!(!html.contains("vrclogger-test-"));
    assert!(!html.contains(fx.dir.path().to_string_lossy().as_ref()));
    assert!(!html.contains("<script>alert"));
    assert!(!html.contains("http://") && !html.contains("https://"));
}

#[test]
fn album_export_embeds_photos_and_counts_skips() {
    let s = setup();
    let out = s.fx.dir.path().join("out").join("album.html");
    let summary = export_album(&s.fx.conn, s.album, &out, &StoryOptions::default(), now()).unwrap();
    assert_eq!((summary.photos_embedded, summary.photos_skipped), (2, 2));
    assert!(!out.with_file_name("album.html.tmp").exists());

    let html = std::fs::read_to_string(&out).unwrap();
    assert_eq!(html.matches("data:image/jpeg;base64,").count(), 2);
    assert!(html.contains("<title>Evening &lt;b&gt;trip&lt;/b&gt;</title>"));
    assert!(html.contains("<h1>Evening &lt;b&gt;trip&lt;/b&gt;</h1>"));
    assert!(html.contains("second &amp; last"));
    assert!(html.contains("2026-11-19")); // date range
    assert!(html.contains("&lt;script&gt;alert(1)&lt;/script&gt; World"));
    assert!(html.contains("2026-11-19 21:15 · "));
    assert!(html.contains("Friend &amp; Co"));
    assert!(html.contains("VrcLogger で書き出し (2026-11-20 01:02)"));
    assert!(html.contains("lang=\"ja\""));
    assert!(html.contains("loading=\"lazy\""));
    assert_private(&html, &s.fx);

    // the photo sources are only read
    assert!(s.fx.dir.path().join("photos").join("ok1.png").exists());
}

#[test]
fn album_export_without_players_omits_names() {
    let s = setup();
    let out = s.fx.dir.path().join("album.html");
    let opts = StoryOptions { include_players: false, max_edge: 64 };
    let summary = export_album(&s.fx.conn, s.album, &out, &opts, now()).unwrap();
    assert_eq!(summary.photos_embedded, 2);
    let html = std::fs::read_to_string(&out).unwrap();
    assert!(!html.contains("Friend"));
    assert!(html.contains("&lt;script&gt;alert(1)&lt;/script&gt; World")); // world names stay
    assert_private(&html, &s.fx);
}

#[test]
fn unknown_album_is_an_error_and_leaves_no_file() {
    let s = setup();
    let out = s.fx.dir.path().join("none.html");
    let r = export_album(&s.fx.conn, 424_242, &out, &StoryOptions::default(), now());
    assert!(matches!(r, Err(Error::Library(LibraryError::AlbumNotFound))));
    assert!(!out.exists());
}

#[test]
fn day_export_has_rail_sections_and_loose_photos() {
    let s = setup();
    let out = s.fx.dir.path().join("day.html");
    let summary = export_day(&s.fx.conn, d(2026, 11, 19), 6, &out, &StoryOptions::default(), now()).unwrap();
    // ok1 (in visit) and ok2 (loose) embedded; nofile skipped; the missing one is not part of the day
    assert_eq!((summary.photos_embedded, summary.photos_skipped), (2, 1));

    let html = std::fs::read_to_string(&out).unwrap();
    assert!(html.contains("2026-11-19 の記録"));
    assert!(html.contains("class=\"rail\""));
    assert!(html.contains("<h2>&lt;script&gt;alert(1)&lt;/script&gt; World</h2>"));
    assert!(html.contains("21:00 – 22:00 · 1h 00m"));
    assert!(html.contains("会った人: Friend &amp; Co"));
    assert_eq!(html.matches("data:image/jpeg;base64,").count(), 2);
    // visit section first, then the loose 23:30 photo
    let visit_at = html.find("<h2>").unwrap();
    let loose_at = html.find("<div class=\"when\">23:30</div>").unwrap();
    assert!(visit_at < loose_at);
    assert!(html.contains("VrcLogger で書き出し (2026-11-20 01:02)"));
    assert_private(&html, &s.fx);

    let opts = StoryOptions { include_players: false, ..StoryOptions::default() };
    export_day(&s.fx.conn, d(2026, 11, 19), 6, &out, &opts, now()).unwrap();
    let html = std::fs::read_to_string(&out).unwrap();
    assert!(!html.contains("Friend"));
    assert!(!html.contains("会った人"));
}

#[test]
fn empty_day_still_exports() {
    let s = setup();
    let out = s.fx.dir.path().join("empty.html");
    let summary = export_day(&s.fx.conn, d(2020, 1, 1), 6, &out, &StoryOptions::default(), now()).unwrap();
    assert_eq!((summary.photos_embedded, summary.photos_skipped), (0, 0));
    let html = std::fs::read_to_string(&out).unwrap();
    assert!(html.contains("2020-01-01 の記録"));
    assert!(html.contains("この日の記録はありません"));
}
