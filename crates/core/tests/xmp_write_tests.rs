mod common;
#[path = "common/xmp_helpers.rs"]
mod xmp_helpers;

use std::fs::{self, File, FileTimes};
use std::path::Path;
use std::time::{Duration, SystemTime};

use common::dt;
use vrclogger_core::photos::{
    build_xmp, insert_xmp, parse_xmp, read_jpeg_xmp, remove_our_xmp_from_file, strip_our_xmp, write_xmp_to_file,
    xmp_presence, XmpFields, XmpPresence,
};
use xmp_helpers::*;

fn fields() -> XmpFields {
    XmpFields {
        creator_tool: Some("VRChat".into()),
        author: Some("Tester".into()),
        author_id: Some(USR.into()),
        create_date: dt(2026, 11, 19, 23, 9, 58),
        create_offset_minutes: 540,
        utc_offset_minutes: 540,
        world_id: WRLD.into(),
        world_name: "Sample World".into(),
        written_at: dt(2026, 11, 21, 12, 0, 0),
    }
}

fn pixels(bytes: &[u8]) -> (u32, u32, Vec<u8>) {
    let img = image::load_from_memory(bytes).unwrap().to_rgb8();
    (img.width(), img.height(), img.into_raw())
}

fn write(dir: &Path, name: &str, bytes: &[u8]) -> std::path::PathBuf {
    let p = dir.join(name);
    fs::write(&p, bytes).unwrap();
    p
}

#[test]
fn png_insert_then_strip_restores_exact_bytes() {
    let orig = png_bytes(40, 30);
    let new = insert_xmp(&orig, &build_xmp(&fields())).unwrap();
    let kinds = png_kinds(&new);
    assert_eq!(&kinds[..2], ["IHDR", "iTXt"]);
    assert_eq!(kinds.len(), png_kinds(&orig).len() + 1);

    // CRC of the new chunk is valid.
    let (_, start, end) = png_layout(&new)[1];
    assert_eq!(u32::from_be_bytes(new[end - 4..end].try_into().unwrap()), crc32(&new[start + 4..end - 4]));
    assert_eq!(pixels(&new), pixels(&orig));
    assert_eq!(strip_our_xmp(&new).unwrap().unwrap(), orig);
    assert!(strip_our_xmp(&orig).unwrap().is_none());
}

#[test]
fn jpeg_insert_then_strip_restores_exact_bytes() {
    let orig = jpeg_bytes(40, 30);
    let xmp = build_xmp(&fields());
    let new = insert_xmp(&orig, &xmp).unwrap();
    assert_eq!(pixels(&new), pixels(&orig));
    assert_eq!(strip_our_xmp(&new).unwrap().unwrap(), orig);
    let got = read_jpeg_xmp(std::io::Cursor::new(&new)).unwrap().unwrap();
    assert_eq!(got, xmp);
    assert!(parse_xmp(&got).ours);
    assert_eq!(read_jpeg_xmp(std::io::Cursor::new(&orig)).unwrap(), None);
}

#[test]
fn jpeg_xmp_goes_after_exif() {
    let base = jpeg_bytes(16, 16);
    let mut exif_payload = b"Exif\0\0".to_vec();
    exif_payload.extend_from_slice(b"II*\0fake-exif-body");
    let orig = jpeg_with_segment_after_soi(&base, &jpeg_app1(&exif_payload));
    let new = insert_xmp(&orig, &build_xmp(&fields())).unwrap();

    let segs = jpeg_segments(&new);
    let exif_at = segs.iter().position(|(m, p)| *m == 0xE1 && p.starts_with(b"Exif\0\0")).unwrap();
    let xmp_at = segs
        .iter()
        .position(|(m, p)| *m == 0xE1 && p.starts_with(b"http://ns.adobe.com/xap/1.0/\0"))
        .unwrap();
    assert!(xmp_at > exif_at);
    // Everything before our segment is APP0 / EXIF only.
    assert!(segs[..xmp_at].iter().all(|(m, _)| *m == 0xE0 || *m == 0xE1));
    let xmp = read_jpeg_xmp(std::io::Cursor::new(&new)).unwrap().unwrap();
    assert!(parse_xmp(&xmp).ours);
    assert_eq!(strip_our_xmp(&new).unwrap().unwrap(), orig);
}

#[test]
fn build_xmp_round_trips_and_escapes() {
    let mut f = fields();
    f.world_name = "A & B <c> \"日本語\" 'x'".into();
    f.author = Some("O'Neil <&>".into());
    let x = build_xmp(&f);
    assert!(!x.contains("xpacket") && !x.starts_with('\u{feff}'));
    assert!(x.contains("2026-11-19T23:09:58+09:00"));
    assert!(x.contains("2026-11-21T12:00:00+09:00"));
    let p = parse_xmp(&x);
    assert!(p.ours);
    assert_eq!(p.world_name.as_deref(), Some("A & B <c> \"日本語\" 'x'"));
    assert_eq!(p.author.as_deref(), Some("O'Neil <&>"));
    assert_eq!(p.author_id.as_deref(), Some(USR));
    assert_eq!(p.world_id.as_deref(), Some(WRLD));
    assert_eq!(p.create_date, Some(f.create_date));

    // The capture time and the time of writing each carry their own offset.
    f.create_offset_minutes = -(5 * 60 + 30);
    let x = build_xmp(&f);
    assert!(x.contains("<xmp:CreateDate>2026-11-19T23:09:58-05:30</xmp:CreateDate>"));
    assert!(x.contains("<vrclogger:WrittenAt>2026-11-21T12:00:00+09:00</vrclogger:WrittenAt>"));
}

#[test]
fn build_xmp_omits_none_fields() {
    let mut f = fields();
    f.creator_tool = None;
    f.author = None;
    f.author_id = None;
    let x = build_xmp(&f);
    assert!(!x.contains("CreatorTool") && !x.contains("xmp:Author") && !x.contains("AuthorID"));
    let p = parse_xmp(&x);
    assert!(p.ours);
    assert_eq!((p.author, p.author_id), (None, None));
    assert_eq!(p.world_id.as_deref(), Some(WRLD));
}

#[test]
fn parse_xmp_without_marker_is_not_ours() {
    assert!(!parse_xmp(&vrchat_xmp("W")).ours);
    assert_eq!(parse_xmp(&vrchat_xmp("W")).author.as_deref(), Some("Someone"));
}

#[test]
fn presence_detection_and_refusal() {
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();
    let png = png_bytes(20, 10);
    let jpg = jpeg_bytes(20, 10);
    let ours_png = insert_xmp(&png, &build_xmp(&fields())).unwrap();
    let ours_jpg = insert_xmp(&jpg, &build_xmp(&fields())).unwrap();
    let iend = png_layout(&png).len() - 1;

    let mut ext = b"http://ns.adobe.com/xmp/extension/\0".to_vec();
    ext.extend_from_slice(&[7u8; 40]);
    let mut std_xmp = b"http://ns.adobe.com/xap/1.0/\0".to_vec();
    std_xmp.extend_from_slice(vrchat_xmp("W").as_bytes());

    let kw = "XML:com.adobe.xmp";
    let mut ztxt = kw.as_bytes().to_vec();
    ztxt.extend_from_slice(&[0, 0, 1, 2, 3]);
    let mut text = kw.as_bytes().to_vec();
    text.push(0);
    text.extend_from_slice(vrchat_xmp("W").as_bytes());

    let cases: Vec<(&str, Vec<u8>, XmpPresence)> = vec![
        ("none.png", png.clone(), XmpPresence::None),
        ("none.jpg", jpg.clone(), XmpPresence::None),
        ("ours.png", ours_png, XmpPresence::Ours),
        ("ours.jpg", ours_jpg, XmpPresence::Ours),
        ("vrchat.png", png_with_chunk(20, 10, &itxt(kw, false, &vrchat_xmp("W"))), XmpPresence::Other),
        ("photoshop.png", png_with_chunk(20, 10, &itxt(kw, false, &photoshop_xmp())), XmpPresence::Other),
        ("after_idat.png", png_insert_at(&png, iend, &itxt(kw, false, &vrchat_xmp("W"))), XmpPresence::Other),
        ("compressed.png", png_with_chunk(20, 10, &itxt(kw, true, "zzzz")), XmpPresence::Other),
        ("ztxt.png", png_with_chunk(20, 10, &chunk(b"zTXt", &ztxt)), XmpPresence::Other),
        ("text.png", png_with_chunk(20, 10, &chunk(b"tEXt", &text)), XmpPresence::Other),
        ("comment.png", png_with_chunk(20, 10, &itxt("Comment", false, "hi")), XmpPresence::None),
        ("foreign.jpg", jpeg_with_segment_after_soi(&jpg, &jpeg_app1(&std_xmp)), XmpPresence::Other),
        ("extended.jpg", jpeg_with_segment_after_soi(&jpg, &jpeg_app1(&ext)), XmpPresence::Other),
        ("exif.jpg", jpeg_with_segment_after_soi(&jpg, &jpeg_app1(b"Exif\0\0II*\0")), XmpPresence::None),
    ];
    for (name, bytes, want) in cases {
        let p = write(d, name, &bytes);
        assert_eq!(xmp_presence(&p).unwrap(), want, "{name}");
        if want != XmpPresence::None {
            assert!(write_xmp_to_file(&p, &fields()).is_err(), "{name} must be refused");
            assert_eq!(fs::read(&p).unwrap(), bytes, "{name} must be untouched");
        }
    }
    assert!(!d.join(".none.png.vrclogger-tmp").exists());
}

#[test]
fn write_preserves_mtime_and_leaves_no_temp_file() {
    let dir = tempfile::tempdir().unwrap();
    for (name, orig) in [("a.png", png_bytes(24, 16)), ("b.jpg", jpeg_bytes(24, 16))] {
        let p = write(dir.path(), name, &orig);
        let old = SystemTime::now() - Duration::from_secs(400 * 24 * 3600);
        File::options().write(true).open(&p).unwrap().set_times(FileTimes::new().set_modified(old)).unwrap();
        let before = fs::metadata(&p).unwrap();

        write_xmp_to_file(&p, &fields()).unwrap();

        let after = fs::metadata(&p).unwrap();
        assert_eq!(after.modified().unwrap(), before.modified().unwrap(), "{name}");
        #[cfg(windows)]
        assert_eq!(after.created().unwrap(), before.created().unwrap(), "{name}");
        assert!(after.len() > before.len());
        assert_eq!(xmp_presence(&p).unwrap(), XmpPresence::Ours);
        let leftovers = fs::read_dir(dir.path())
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().contains("vrclogger-tmp"))
            .count();
        assert_eq!(leftovers, 0);

        // And back again, also keeping the time.
        assert!(remove_our_xmp_from_file(&p).unwrap());
        assert_eq!(fs::read(&p).unwrap(), orig);
        assert_eq!(fs::metadata(&p).unwrap().modified().unwrap(), before.modified().unwrap());
        assert!(!remove_our_xmp_from_file(&p).unwrap());
    }
}

#[test]
fn read_only_file_is_refused_and_unchanged() {
    let dir = tempfile::tempdir().unwrap();
    let orig = png_bytes(8, 8);
    let p = write(dir.path(), "ro.png", &orig);
    let mut perm = fs::metadata(&p).unwrap().permissions();
    perm.set_readonly(true);
    fs::set_permissions(&p, perm.clone()).unwrap();

    let err = write_xmp_to_file(&p, &fields()).unwrap_err().to_string();
    assert!(err.contains("read-only"), "{err}");
    assert!(remove_our_xmp_from_file(&p).is_err());
    assert_eq!(fs::read(&p).unwrap(), orig);

    // So the temp dir can be cleaned up.
    #[allow(clippy::permissions_set_readonly_false)]
    perm.set_readonly(false);
    fs::set_permissions(&p, perm).unwrap();
}

#[test]
fn strip_never_removes_foreign_xmp() {
    let foreign = png_with_chunk(12, 12, &itxt("XML:com.adobe.xmp", false, &vrchat_xmp("W")));
    assert!(strip_our_xmp(&foreign).unwrap().is_none());
    let with_both = insert_xmp(&foreign, &build_xmp(&fields())).unwrap();
    assert_eq!(strip_our_xmp(&with_both).unwrap().unwrap(), foreign);

    let mut std_xmp = b"http://ns.adobe.com/xap/1.0/\0".to_vec();
    std_xmp.extend_from_slice(vrchat_xmp("W").as_bytes());
    let jf = jpeg_with_segment_after_soi(&jpeg_bytes(12, 12), &jpeg_app1(&std_xmp));
    assert!(strip_our_xmp(&jf).unwrap().is_none());
    let jboth = insert_xmp(&jf, &build_xmp(&fields())).unwrap();
    assert_eq!(strip_our_xmp(&jboth).unwrap().unwrap(), jf);

    // remove_our_xmp_from_file on a foreign-only file does nothing.
    let dir = tempfile::tempdir().unwrap();
    let p = write(dir.path(), "f.png", &foreign);
    assert!(!remove_our_xmp_from_file(&p).unwrap());
    assert_eq!(fs::read(&p).unwrap(), foreign);
}

#[test]
fn unsupported_or_broken_input_is_an_error_and_writes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let mut webp = b"RIFF".to_vec();
    webp.extend_from_slice(&[0x20, 0, 0, 0]);
    webp.extend_from_slice(b"WEBPVP8 ");
    webp.extend_from_slice(&[0u8; 32]);
    let random: Vec<u8> = (0..200u32).map(|i| (i * 31 % 251) as u8).collect();
    let full = png_bytes(10, 10);
    let truncated_png = full[..40].to_vec();
    let no_iend = full[..full.len() - 12].to_vec();
    let truncated_jpg = jpeg_bytes(10, 10)[..30].to_vec();

    let x = build_xmp(&fields());
    for (name, bytes) in [
        ("a.webp", webp),
        ("b.png", random),
        ("c.png", truncated_png),
        ("d.png", no_iend),
        ("e.jpg", truncated_jpg),
        ("f.png", Vec::new()),
    ] {
        assert!(insert_xmp(&bytes, &x).is_err(), "{name}");
        let p = write(dir.path(), name, &bytes);
        assert!(write_xmp_to_file(&p, &fields()).is_err(), "{name}");
        assert_eq!(fs::read(&p).unwrap(), bytes, "{name}");
    }
    assert!(strip_our_xmp(b"not an image at all").is_err());
}

#[test]
fn oversized_jpeg_payload_is_rejected() {
    let jpg = jpeg_bytes(8, 8);
    let big = "x".repeat(65_534 - 29);
    assert!(insert_xmp(&jpg, &big).is_err());
    let ok = "x".repeat(65_533 - 29);
    let new = insert_xmp(&jpg, &ok).unwrap();
    assert_eq!(new.len(), jpg.len() + 4 + 65_533);
}

/// Manual probe on copies of real photos: `PROBE_DIR=<dir of copies> cargo test -- --ignored probe_real_copies --nocapture`.
/// Writes, checks, removes, and compares with the original bytes. Never point it at originals.
#[test]
#[ignore]
fn probe_real_copies() {
    use vrclogger_core::photos::{remove_our_xmp_from_file, write_xmp_to_file, xmp_presence, XmpFields, XmpPresence};
    let dir = std::env::var("PROBE_DIR").unwrap();
    let ts = chrono::NaiveDate::from_ymd_opt(2026, 1, 10).unwrap().and_hms_opt(20, 30, 0).unwrap();
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        let before = std::fs::read(&path).unwrap();
        let mtime = std::fs::metadata(&path).unwrap().modified().unwrap();
        let presence = xmp_presence(&path).unwrap();
        let fields = XmpFields {
            creator_tool: Some("VRChat".into()),
            author: Some("Probe Author".into()),
            author_id: Some("usr_11111111-1111-1111-1111-111111111111".into()),
            create_date: ts,
            create_offset_minutes: 540,
            utc_offset_minutes: 540,
            world_id: "wrld_11111111-1111-1111-1111-111111111111".into(),
            world_name: "Probe & <World> 日本語".into(),
            written_at: ts,
        };
        if let Err(e) = write_xmp_to_file(&path, &fields) {
            assert_eq!(std::fs::read(&path).unwrap(), before, "{name}: refused but changed");
            eprintln!("{name}: presence={presence:?} -> refused ({e}), file unchanged");
            continue;
        }
        let after = std::fs::read(&path).unwrap();
        assert_eq!(xmp_presence(&path).unwrap(), XmpPresence::Ours);
        assert_eq!(std::fs::metadata(&path).unwrap().modified().unwrap(), mtime, "{name}: mtime changed");
        let a = image::load_from_memory(&before).map(|i| i.to_rgb8());
        let b = image::load_from_memory(&after).map(|i| i.to_rgb8());
        let same_pixels = match (a, b) {
            (Ok(a), Ok(b)) => a == b,
            _ => false,
        };
        assert!(remove_our_xmp_from_file(&path).unwrap());
        assert_eq!(std::fs::read(&path).unwrap(), before, "{name}: not restored");
        eprintln!("{name}: presence={presence:?} -> written (+{} bytes), pixels same={same_pixels}, restored byte-identical", after.len() - before.len());
    }
}
