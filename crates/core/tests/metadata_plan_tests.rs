mod common;
#[path = "common/xmp_helpers.rs"]
mod xmp_helpers;

use std::fs;
use std::path::{Path, PathBuf};

use common::{dt, import_visits, Fixture, LOCAL_USR, V};
use yorimichi_core::data::metadata::{MetaOutcome, MetaTarget, MetadataService, SkipReason, XmpState};
use yorimichi_core::photos::scan_folders;
use yorimichi_core::PhotoError;
use xmp_helpers::*;

const OFFSET: i32 = 540;

fn now() -> chrono::NaiveDateTime {
    dt(2026, 11, 21, 12, 0, 0)
}

fn photos_dir(fx: &Fixture) -> PathBuf {
    fx.dir.path().join("photos")
}

fn put(fx: &Fixture, name: &str, bytes: &[u8]) -> PathBuf {
    let dir = photos_dir(fx);
    fs::create_dir_all(&dir).unwrap();
    let p = dir.join(name);
    fs::write(&p, bytes).unwrap();
    p
}

fn scan(fx: &Fixture) {
    scan_folders(&fx.conn, &[photos_dir(fx)], None).unwrap();
}

fn id_of(fx: &Fixture, name: &str) -> i64 {
    fx.conn.query_row("SELECT id FROM photos WHERE file_name = ?1", [name], |r| r.get(0)).unwrap()
}

/// One visit on 2026-01-10 20:00-21:00 in a world with a dummy id.
fn with_visit(fx: &Fixture) {
    import_visits(fx, &[V::new("2026.01.10", "20:00:00", "21:00:00", "Sample World", WRLD)]);
}

fn reason(svc: &MetadataService, id: i64) -> SkipReason {
    svc.classify_write(id, OFFSET, now()).unwrap().unwrap_err()
}

#[test]
fn eligible_photo_gets_the_right_fields() {
    let fx = Fixture::new();
    with_visit(&fx);
    put(&fx, "VRChat_2026-01-10_20-30-00.000_100x50.png", &png_bytes(10, 8));
    put(&fx, "vrc_2026-01-10_20-31-00.jpg", &jpeg_bytes(10, 8));
    put(&fx, "shot_2026-01-10_20-32-00.png", &png_bytes(10, 8));
    scan(&fx);
    let svc = MetadataService::new(&fx.conn);

    let (path, f) = svc
        .classify_write(id_of(&fx, "VRChat_2026-01-10_20-30-00.000_100x50.png"), OFFSET, now())
        .unwrap()
        .unwrap();
    assert!(path.ends_with("VRChat_2026-01-10_20-30-00.000_100x50.png"));
    assert_eq!(f.world_id, WRLD);
    assert_eq!(f.world_name, "Sample World");
    assert_eq!(f.creator_tool.as_deref(), Some("VRChat"));
    assert_eq!(f.author_id.as_deref(), Some(LOCAL_USR));
    assert_eq!(f.author.as_deref(), Some("Tester"));
    assert_eq!(f.create_date, dt(2026, 1, 10, 20, 30, 0));
    assert_eq!((f.utc_offset_minutes, f.written_at), (OFFSET, now()));
    assert_eq!(f.create_offset_minutes, OFFSET);

    // With a capture-offset rule, the capture time gets the offset of its own date.
    let dst = MetadataService::new(&fx.conn).with_capture_offset(|t| if t < dt(2026, 3, 1, 0, 0, 0) { -480 } else { -420 });
    let (_, f) = dst.classify_write(id_of(&fx, "vrc_2026-01-10_20-31-00.jpg"), -420, now()).unwrap().unwrap();
    assert_eq!((f.create_offset_minutes, f.utc_offset_minutes), (-480, -420));

    let (_, f) = svc.classify_write(id_of(&fx, "vrc_2026-01-10_20-31-00.jpg"), OFFSET, now()).unwrap().unwrap();
    assert_eq!(f.creator_tool.as_deref(), Some("VRChat"));
    let (_, f) = svc.classify_write(id_of(&fx, "shot_2026-01-10_20-32-00.png"), OFFSET, now()).unwrap().unwrap();
    assert_eq!(f.creator_tool, None);
}

#[test]
fn every_skip_reason_comes_from_its_situation() {
    let fx = Fixture::new();
    with_visit(&fx);
    put(&fx, "VRChat_2026-01-10_20-30-00.000.png", &png_bytes(6, 6)); // eligible
    put(&fx, "plain.png", &png_bytes(6, 6)); // no time in the name -> mtime
    put(&fx, "VRChat_2026-01-11_10-00-00.000.png", &png_bytes(6, 6)); // outside any visit
    // Already carries XMP; its CreateDate is inside the visit.
    let xmp = vrchat_xmp("Other World");
    put(&fx, "has_xmp.png", &png_with_chunk(6, 6, &itxt("XML:com.adobe.xmp", false, &xmp)));
    put(&fx, "shot_2026-01-10_20-33-00.webp", b"RIFF\x08\0\0\0WEBPVP8 ");
    put(&fx, "gone_2026-01-10_20-34-00.png", &png_bytes(6, 6));
    let ro = put(&fx, "ro_2026-01-10_20-35-00.png", &png_bytes(6, 6));
    scan(&fx);
    let svc = MetadataService::new(&fx.conn);

    fs::remove_file(photos_dir(&fx).join("gone_2026-01-10_20-34-00.png")).unwrap();
    let mut perm = fs::metadata(&ro).unwrap().permissions();
    perm.set_readonly(true);
    fs::set_permissions(&ro, perm.clone()).unwrap();

    assert!(svc.classify_write(id_of(&fx, "VRChat_2026-01-10_20-30-00.000.png"), OFFSET, now()).unwrap().is_ok());
    assert_eq!(reason(&svc, id_of(&fx, "plain.png")), SkipReason::EstimatedTime);
    assert_eq!(reason(&svc, id_of(&fx, "VRChat_2026-01-11_10-00-00.000.png")), SkipReason::NoLog);
    assert_eq!(reason(&svc, id_of(&fx, "has_xmp.png")), SkipReason::HasMetadata);
    assert_eq!(reason(&svc, id_of(&fx, "shot_2026-01-10_20-33-00.webp")), SkipReason::UnsupportedFormat);
    assert_eq!(reason(&svc, id_of(&fx, "gone_2026-01-10_20-34-00.png")), SkipReason::Missing);
    assert_eq!(reason(&svc, id_of(&fx, "ro_2026-01-10_20-35-00.png")), SkipReason::ReadOnly);
    assert_eq!(reason(&svc, 99_999), SkipReason::Missing);

    // Online-only is detected from the DB flag without opening the file.
    let eligible = id_of(&fx, "VRChat_2026-01-10_20-30-00.000.png");
    fx.conn.execute("UPDATE photos SET online_only = 1 WHERE id = ?1", [eligible]).unwrap();
    assert_eq!(reason(&svc, eligible), SkipReason::OnlineOnly);
    assert_eq!(svc.classify_remove(eligible).unwrap().unwrap_err(), SkipReason::OnlineOnly);
    assert_eq!(svc.apply_write(eligible, OFFSET, now()).unwrap(), MetaOutcome::Skipped(SkipReason::OnlineOnly));

    #[allow(clippy::permissions_set_readonly_false)]
    perm.set_readonly(false);
    fs::set_permissions(&ro, perm).unwrap();
}

#[test]
fn plan_write_counts_and_example() {
    let fx = Fixture::new();
    with_visit(&fx);
    put(&fx, "plain.png", &png_bytes(6, 6));
    put(&fx, "VRChat_2026-01-10_20-30-00.000.png", &png_bytes(6, 6));
    put(&fx, "vrc_2026-01-10_20-31-00.jpg", &jpeg_bytes(6, 6));
    put(&fx, "VRChat_2026-01-11_10-00-00.000.png", &png_bytes(6, 6));
    put(&fx, "VRChat_2026-01-12_10-00-00.000.png", &png_bytes(6, 6));
    put(&fx, "x_2026-01-10_20-33-00.webp", b"RIFF");
    scan(&fx);
    let svc = MetadataService::new(&fx.conn);

    let ids = svc.resolve(&MetaTarget::All).unwrap();
    assert_eq!(ids.len(), 6);
    let plan = svc.plan_write(&ids, OFFSET, now()).unwrap();
    assert_eq!((plan.total, plan.eligible), (6, 2));
    assert_eq!(plan.eligible_ids.len(), 2);
    let counts: Vec<_> = plan.skipped.iter().map(|s| (s.reason, s.count)).collect();
    // Declaration order: NoLog, EstimatedTime, UnsupportedFormat; zero counts are left out.
    assert_eq!(
        counts,
        vec![(SkipReason::NoLog, 2), (SkipReason::EstimatedTime, 1), (SkipReason::UnsupportedFormat, 1)]
    );
    let ex = plan.example.clone().unwrap();
    assert_eq!(ex.file_name, "VRChat_2026-01-10_20-30-00.000.png");
    assert_eq!(ex.world_id, WRLD);
    assert_eq!(ex.world_name, "Sample World");
    assert_eq!(ex.taken_at, "2026-01-10 20:30:00");
    assert_eq!(ex.author.as_deref(), Some("Tester"));

    let json = serde_json::to_value(&plan).unwrap();
    assert_eq!(json["eligibleIds"].as_array().unwrap().len(), 2);
    assert_eq!(json["skipped"][0]["reason"], "noLog");
    assert_eq!(json["example"]["fileName"], "VRChat_2026-01-10_20-30-00.000.png");
}

#[test]
fn apply_write_then_remove_round_trips() {
    let fx = Fixture::new();
    with_visit(&fx);
    let png = png_bytes(12, 9);
    let jpg = jpeg_bytes(12, 9);
    let p1 = put(&fx, "VRChat_2026-01-10_20-30-00.000.png", &png);
    let p2 = put(&fx, "vrc_2026-01-10_20-31-00.jpg", &jpg);
    scan(&fx);
    let svc = MetadataService::new(&fx.conn);
    let ids = [id_of(&fx, "VRChat_2026-01-10_20-30-00.000.png"), id_of(&fx, "vrc_2026-01-10_20-31-00.jpg")];
    fx.conn.execute("UPDATE photos SET favorite = 1 WHERE id = ?1", [ids[0]]).unwrap();
    assert_eq!(svc.plan_remove(&ids).unwrap().eligible, 0); // nothing of ours yet

    for id in ids {
        assert_eq!(svc.apply_write(id, OFFSET, now()).unwrap(), MetaOutcome::Done);
    }
    assert_ne!(fs::read(&p1).unwrap(), png);
    assert_ne!(fs::read(&p2).unwrap(), jpg);

    let (world, source, size, fav): (String, String, i64, bool) = fx
        .conn
        .query_row(
            "SELECT xmp_world_id, time_source, file_size, favorite FROM photos WHERE id = ?1",
            [ids[0]],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .unwrap();
    assert_eq!(world, WRLD);
    assert_eq!(source, "xmp");
    assert_eq!(size as usize, fs::read(&p1).unwrap().len());
    assert!(fav, "favorite survives the refresh");
    let author: String =
        fx.conn.query_row("SELECT xmp_author_id FROM photos WHERE id = ?1", [ids[1]], |r| r.get(0)).unwrap();
    assert_eq!(author, LOCAL_USR);

    // A second plan sees the metadata; applying again skips.
    let plan = svc.plan_write(&ids, OFFSET, now()).unwrap();
    assert_eq!(plan.eligible, 0);
    assert_eq!(plan.skipped[0].reason, SkipReason::HasMetadata);
    assert_eq!(plan.skipped[0].count, 2);
    assert_eq!(svc.apply_write(ids[0], OFFSET, now()).unwrap(), MetaOutcome::Skipped(SkipReason::HasMetadata));

    let rp = svc.plan_remove(&ids).unwrap();
    assert_eq!((rp.eligible, rp.total), (2, 2));
    assert!(rp.example.is_none());
    for id in ids {
        assert_eq!(svc.apply_remove(id).unwrap(), MetaOutcome::Done);
    }
    assert_eq!(fs::read(&p1).unwrap(), png);
    assert_eq!(fs::read(&p2).unwrap(), jpg);
    let (world, source): (Option<String>, String) = fx
        .conn
        .query_row("SELECT xmp_world_id, time_source FROM photos WHERE id = ?1", [ids[0]], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap();
    assert_eq!((world, source.as_str()), (None, "name"));
    assert_eq!(svc.apply_remove(ids[0]).unwrap(), MetaOutcome::Skipped(SkipReason::HasMetadata));
    // The pre-write file can be written again.
    assert_eq!(svc.plan_write(&ids, OFFSET, now()).unwrap().eligible, 2);
}

#[test]
fn apply_failure_is_reported_not_raised() {
    let fx = Fixture::new();
    with_visit(&fx);
    // Valid extension and no XMP, but the content is not an image.
    let p = put(&fx, "VRChat_2026-01-10_20-30-00.000.png", b"this is not a png at all");
    scan(&fx);
    let svc = MetadataService::new(&fx.conn);
    let id = id_of(&fx, "VRChat_2026-01-10_20-30-00.000.png");
    assert!(svc.classify_write(id, OFFSET, now()).unwrap().is_ok());
    assert!(matches!(
        svc.apply_write(id, OFFSET, now()).unwrap(),
        MetaOutcome::Failed(PhotoError::MalformedPng(_) | PhotoError::UnsupportedFormat)
    ));
    assert_eq!(fs::read(&p).unwrap(), b"this is not a png at all");
}

fn resolve_fixture() -> (Fixture, Vec<(&'static str, i64)>) {
    let fx = Fixture::new();
    let names = [
        "shot_2026-01-09_12-00-00.png",
        "shot_2026-01-10_05-59-59.png",
        "shot_2026-01-10_06-00-00.png",
        "shot_2026-01-11_03-00-00.png",
        "shot_2026-01-11_06-00-00.png",
        "shot_2026-01-12_23-59-59.png",
    ];
    for n in names {
        put(&fx, n, &png_bytes(4, 4));
    }
    scan(&fx);
    let ids = names.iter().map(|n| (*n, id_of(&fx, n))).collect();
    (fx, ids)
}

#[test]
fn resolve_targets() {
    let (fx, ids) = resolve_fixture();
    let svc = MetadataService::new(&fx.conn);
    let id = |i: usize| ids[i].1;

    // Ids: given order, unknown and repeated ids dropped.
    let got = svc.resolve(&MetaTarget::Ids { ids: vec![id(3), 99_999, id(0), id(3)] }).unwrap();
    assert_eq!(got, vec![id(3), id(0)]);
    // All: time order.
    assert_eq!(svc.resolve(&MetaTarget::All).unwrap(), ids.iter().map(|(_, i)| *i).collect::<Vec<_>>());
}

#[test]
fn resolve_all_skips_missing_rows() {
    let (fx, ids) = resolve_fixture();
    fx.conn.execute("UPDATE photos SET missing = 1 WHERE id = ?1", [ids[1].1]).unwrap();
    let svc = MetadataService::new(&fx.conn);
    let got = svc.resolve(&MetaTarget::All).unwrap();
    let want: Vec<i64> = ids.iter().enumerate().filter(|(i, _)| *i != 1).map(|(_, (_, id))| *id).collect();
    assert_eq!(got, want);
}

/// Online-only rows are simulated with the DB flag (real placeholders need a sync client).
fn mark_online(fx: &Fixture, id: i64) {
    fx.conn.execute("UPDATE photos SET online_only = 1 WHERE id = ?1", [id]).unwrap();
}

#[test]
fn plan_write_puts_online_only_candidates_in_the_download_bucket() {
    let fx = Fixture::new();
    with_visit(&fx);
    let png = png_bytes(6, 6);
    put(&fx, "VRChat_2026-01-10_20-30-00.000.png", &png); // local, eligible
    put(&fx, "VRChat_2026-01-10_20-31-00.000.png", &png); // online, candidate
    put(&fx, "VRChat_2026-01-11_10-00-00.000.png", &png); // online, no log
    put(&fx, "plain.png", &png); // online, estimated time
    // Online, and the library already read XMP from it before it went online-only.
    put(&fx, "has_xmp.png", &png_with_chunk(6, 6, &itxt("XML:com.adobe.xmp", false, &vrchat_xmp("W"))));
    scan(&fx);
    for n in ["VRChat_2026-01-10_20-31-00.000.png", "VRChat_2026-01-11_10-00-00.000.png", "plain.png", "has_xmp.png"] {
        mark_online(&fx, id_of(&fx, n));
    }
    let svc = MetadataService::new(&fx.conn);
    let ids = svc.resolve(&MetaTarget::All).unwrap();
    let plan = svc.plan_write(&ids, OFFSET, now()).unwrap();

    assert_eq!((plan.total, plan.eligible, plan.online, plan.download), (5, 1, 4, 1));
    assert_eq!(plan.download_ids, vec![id_of(&fx, "VRChat_2026-01-10_20-31-00.000.png")]);
    assert_eq!(plan.download_bytes, png.len() as i64);
    let counts: Vec<_> = plan.skipped.iter().map(|s| (s.reason, s.count)).collect();
    assert_eq!(
        counts,
        vec![(SkipReason::HasMetadata, 1), (SkipReason::NoLog, 1), (SkipReason::EstimatedTime, 1)]
    );
    let json = serde_json::to_value(&plan).unwrap();
    assert_eq!((json["online"].as_u64(), json["download"].as_u64()), (Some(4), Some(1)));
    assert_eq!(json["downloadBytes"].as_i64(), Some(png.len() as i64));
}

#[test]
fn apply_write_downloading_writes_and_records_files_flagged_online() {
    let fx = Fixture::new();
    with_visit(&fx);
    let png = png_bytes(8, 8);
    let p = put(&fx, "VRChat_2026-01-10_20-31-00.000.png", &png);
    let q = put(&fx, "VRChat_2026-01-10_20-32-00.000.png", &png_with_chunk(8, 8, &itxt("XML:com.adobe.xmp", false, &vrchat_xmp("W"))));
    scan(&fx);
    let (a, b) = (id_of(&fx, "VRChat_2026-01-10_20-31-00.000.png"), id_of(&fx, "VRChat_2026-01-10_20-32-00.000.png"));
    // `b` went online-only before its XMP was ever read.
    fx.conn.execute("UPDATE photos SET xmp_world_id = NULL, xmp_author_id = NULL, time_source = 'name' WHERE id = ?1", [b]).unwrap();
    mark_online(&fx, a);
    mark_online(&fx, b);
    let svc = MetadataService::new(&fx.conn);
    let before_q = fs::read(&q).unwrap();

    // The plain write refuses; the downloading one writes (the files here are not placeholders,
    // so nothing is downloaded).
    assert_eq!(svc.apply_write(a, OFFSET, now()).unwrap(), MetaOutcome::Skipped(SkipReason::OnlineOnly));
    let r = svc.apply_write_downloading(a, OFFSET, now()).unwrap();
    assert_eq!(r.outcome, MetaOutcome::Done);
    assert!(!r.downloaded);
    assert_ne!(fs::read(&p).unwrap(), png);
    let (online, world): (bool, Option<String>) = fx
        .conn
        .query_row("SELECT online_only, xmp_world_id FROM photos WHERE id = ?1", [a], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap();
    assert_eq!((online, world.as_deref()), (false, Some(WRLD)));

    // Already has XMP once read: untouched, and the library now knows it.
    let r = svc.apply_write_downloading(b, OFFSET, now()).unwrap();
    assert_eq!(r.outcome, MetaOutcome::Skipped(SkipReason::HasMetadata));
    assert_eq!(fs::read(&q).unwrap(), before_q);
    mark_online(&fx, b);
    let plan = svc.plan_write(&[b], OFFSET, now()).unwrap();
    assert_eq!((plan.download, plan.skipped[0].reason), (0, SkipReason::HasMetadata));

    // Not a candidate without its content (no log): skipped before any read.
    let c = {
        put(&fx, "VRChat_2026-01-11_10-00-00.000.png", &png);
        scan(&fx);
        id_of(&fx, "VRChat_2026-01-11_10-00-00.000.png")
    };
    mark_online(&fx, c);
    assert_eq!(svc.apply_write_downloading(c, OFFSET, now()).unwrap().outcome, MetaOutcome::Skipped(SkipReason::NoLog));
}

#[test]
fn target_deserializes_from_camel_case_json() {
    let t: MetaTarget = serde_json::from_str(r#"{"kind":"all"}"#).unwrap();
    assert!(matches!(t, MetaTarget::All));
    let t: MetaTarget = serde_json::from_str(r#"{"kind":"ids","ids":[1,2]}"#).unwrap();
    assert!(matches!(t, MetaTarget::Ids { ids } if ids == [1, 2]));
    // Removed kinds are rejected.
    assert!(serde_json::from_str::<MetaTarget>(r#"{"kind":"album","albumId":3}"#).is_err());
    assert!(serde_json::from_str::<MetaTarget>(r#"{"kind":"range","from":"2026-01-01","to":"2026-01-31"}"#).is_err());
    assert!(serde_json::from_str::<MetaTarget>(r#"{"kind":"day","day":"2026-01-10"}"#).is_err());
}

#[test]
fn status_reports_each_situation() {
    let fx = Fixture::new();
    with_visit(&fx);
    let png = png_bytes(6, 6);
    put(&fx, "VRChat_2026-01-10_20-30-00.000.png", &png); // writable
    put(&fx, "vrc_2026-01-10_20-31-00.png", &png_with_chunk(6, 6, &itxt("XML:com.adobe.xmp", false, &vrchat_xmp("W")))); // other XMP
    put(&fx, "VRChat_2026-01-10_20-32-00.000.png", &png); // will carry ours
    put(&fx, "VRChat_2026-01-10_20-33-00.000.png", &png); // online-only candidate
    put(&fx, "VRChat_2026-01-11_10-00-00.000.png", &png); // no log
    put(&fx, "gone_2026-01-10_20-34-00.png", &png); // missing
    scan(&fx);
    let svc = MetadataService::new(&fx.conn);
    let st = |n: &str| svc.status(id_of(&fx, n), OFFSET, now()).unwrap();

    let s = st("VRChat_2026-01-10_20-30-00.000.png");
    assert_eq!((s.xmp, s.writable, s.writable_after_download, s.reason, s.removable), (XmpState::None, true, false, None, false));

    let s = st("vrc_2026-01-10_20-31-00.png");
    assert_eq!((s.xmp, s.writable, s.reason, s.removable), (XmpState::Other, false, Some(SkipReason::HasMetadata), false));

    let ours = id_of(&fx, "VRChat_2026-01-10_20-32-00.000.png");
    assert_eq!(svc.apply_write(ours, OFFSET, now()).unwrap(), MetaOutcome::Done);
    let s = svc.status(ours, OFFSET, now()).unwrap();
    assert_eq!((s.xmp, s.writable, s.reason, s.removable), (XmpState::Ours, false, Some(SkipReason::HasMetadata), true));

    mark_online(&fx, id_of(&fx, "VRChat_2026-01-10_20-33-00.000.png"));
    let s = st("VRChat_2026-01-10_20-33-00.000.png");
    assert_eq!((s.xmp, s.writable, s.writable_after_download, s.reason), (XmpState::Unknown, false, true, None));

    let s = st("VRChat_2026-01-11_10-00-00.000.png");
    assert_eq!((s.xmp, s.writable, s.reason), (XmpState::None, false, Some(SkipReason::NoLog)));

    fs::remove_file(photos_dir(&fx).join("gone_2026-01-10_20-34-00.png")).unwrap();
    let s = st("gone_2026-01-10_20-34-00.png");
    assert_eq!((s.xmp, s.writable, s.reason), (XmpState::Unknown, false, Some(SkipReason::Missing)));

    let json = serde_json::to_value(svc.status(ours, OFFSET, now()).unwrap()).unwrap();
    assert_eq!(json["xmp"], "ours");
    assert_eq!(json["writableAfterDownload"], false);
    assert_eq!(json["reason"], "hasMetadata");
}

#[test]
fn refresh_file_marks_vanished_files_missing() {
    let fx = Fixture::new();
    let p = put(&fx, "a_2026-01-10_20-30-00.png", &png_bytes(4, 4));
    scan(&fx);
    let id = id_of(&fx, "a_2026-01-10_20-30-00.png");
    assert!(yorimichi_core::photos::refresh_file(&fx.conn, &p).unwrap());
    fs::remove_file(&p).unwrap();
    assert!(!yorimichi_core::photos::refresh_file(&fx.conn, Path::new(&p)).unwrap());
    let missing: bool = fx.conn.query_row("SELECT missing FROM photos WHERE id = ?1", [id], |r| r.get(0)).unwrap();
    assert!(missing);
}
