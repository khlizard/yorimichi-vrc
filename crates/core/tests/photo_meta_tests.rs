mod common;

use std::io::Cursor;

use common::dt;
use yorimichi_core::photos::{choose_time, parse_xmp, read_meta, read_png_info, time_from_name, TimeSource};

fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &b in data {
        crc ^= u32::from(b);
        for _ in 0..8 {
            crc = if crc & 1 != 0 { (crc >> 1) ^ 0xEDB8_8320 } else { crc >> 1 };
        }
    }
    !crc
}

fn itxt_chunk(keyword: &str, text: &str) -> Vec<u8> {
    let mut data = Vec::new();
    data.extend_from_slice(keyword.as_bytes());
    data.extend_from_slice(&[0, 0, 0, 0, 0]); // nul, compression flag, method, empty lang + nul, empty translated + nul
    data.extend_from_slice(text.as_bytes());
    let mut body = b"iTXt".to_vec();
    body.extend_from_slice(&data);
    let mut chunk = (data.len() as u32).to_be_bytes().to_vec();
    chunk.extend_from_slice(&body);
    chunk.extend_from_slice(&crc32(&body).to_be_bytes());
    chunk
}

/// A real PNG of the given size with an extra iTXt chunk spliced in right after IHDR.
fn png_with_itxt(w: u32, h: u32, keyword: &str, text: &str) -> Vec<u8> {
    let mut buf = Vec::new();
    image::RgbImage::new(w, h)
        .write_to(&mut Cursor::new(&mut buf), image::ImageFormat::Png)
        .unwrap();
    let ihdr_end = 8 + 4 + 4 + 13 + 4;
    let mut out = buf[..ihdr_end].to_vec();
    out.extend(itxt_chunk(keyword, text));
    out.extend_from_slice(&buf[ihdr_end..]);
    out
}

const XMP: &str = "<x:xmpmeta><rdf:Description>\
    <xmp:CreateDate>2026-01-10T20:30:00.1234567+09:00</xmp:CreateDate>\
    <vrc:WorldID>wrld_11111111-1111-1111-1111-111111111111</vrc:WorldID>\
    <vrc:WorldDisplayName>Sample &amp; World</vrc:WorldDisplayName>\
    <vrc:AuthorID>usr_11111111-1111-1111-1111-111111111111</vrc:AuthorID>\
    </rdf:Description></x:xmpmeta>";

#[test]
fn file_name_patterns() {
    assert_eq!(
        time_from_name("VRChat_2026-01-10_20-30-00.123_3840x2160.png"),
        Some(dt(2026, 1, 10, 20, 30, 0))
    );
    assert_eq!(time_from_name("vrc_2024-06-10_18-30-45_123.jpg"), Some(dt(2024, 6, 10, 18, 30, 45)));
    assert_eq!(
        time_from_name("VRChat_1920x1080_2022-01-02_03-04-05.678.png"),
        Some(dt(2022, 1, 2, 3, 4, 5))
    );
    assert_eq!(time_from_name("2024-06-10 18:30:45.png"), Some(dt(2024, 6, 10, 18, 30, 45)));
    assert_eq!(time_from_name("20240610_183045.jpg"), Some(dt(2024, 6, 10, 18, 30, 45)));
    assert_eq!(time_from_name("IMG_20240610-183045.jpg"), Some(dt(2024, 6, 10, 18, 30, 45)));
    assert_eq!(
        time_from_name("ChatGPT Image 2025年6月9日 7_05_03.png"),
        Some(dt(2025, 6, 9, 7, 5, 3))
    );
    assert_eq!(time_from_name("holiday.png"), None);
}

#[test]
fn invalid_dates_and_digit_boundaries() {
    assert_eq!(time_from_name("VRChat_2026-13-20_20-30-00.png"), None);
    assert_eq!(time_from_name("VRChat_2026-11-31_20-30-00.png"), None);
    assert_eq!(time_from_name("20241310_183045.jpg"), None);
    assert_eq!(time_from_name("120240610_183045.jpg"), None); // digit before
    assert_eq!(time_from_name("20240610_1830451.jpg"), None); // digit after
    // An invalid first match does not hide a later valid one.
    assert_eq!(
        time_from_name("2026-13-20_20-30-00_2026-01-02_03-04-05.png"),
        Some(dt(2026, 1, 2, 3, 4, 5))
    );
}

#[test]
fn parse_xmp_fields_and_entities() {
    let x = parse_xmp(XMP);
    assert_eq!(x.create_date, Some(dt(2026, 1, 10, 20, 30, 0)));
    assert_eq!(x.world_id.as_deref(), Some("wrld_11111111-1111-1111-1111-111111111111"));
    assert_eq!(x.world_name.as_deref(), Some("Sample & World"));
    assert_eq!(x.author_id.as_deref(), Some("usr_11111111-1111-1111-1111-111111111111"));

    let x = parse_xmp("<vrc:WorldDisplayName>&lt;a&gt; &quot;b&quot; &apos;c&apos;</vrc:WorldDisplayName>");
    assert_eq!(x.world_name.as_deref(), Some("<a> \"b\" 'c'"));
}

#[test]
fn parse_xmp_reads_attribute_form() {
    // An image editor rewrites the packet with the values as attributes of rdf:Description.
    let xml = r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF><rdf:Description rdf:about=""
        xmlns:xmp="http://ns.adobe.com/xap/1.0/" xmlns:vrc="http://ns.vrchat.com/vrc/1.0/"
        xmp:CreateDate="2026-01-10T20:30:00+09:00" vrc:WorldID="wrld_11111111-1111-1111-1111-111111111111"
        vrc:WorldDisplayName="Sample &amp; &lt;World&gt; &quot;1&quot;" vrc:AuthorID='usr_11111111-1111-1111-1111-111111111111'>
        </rdf:Description></rdf:RDF></x:xmpmeta>"#;
    let x = parse_xmp(xml);
    assert_eq!(x.create_date, Some(dt(2026, 1, 10, 20, 30, 0)));
    assert_eq!(x.world_id.as_deref(), Some("wrld_11111111-1111-1111-1111-111111111111"));
    assert_eq!(x.world_name.as_deref(), Some("Sample & <World> \"1\""));
    assert_eq!(x.author_id.as_deref(), Some("usr_11111111-1111-1111-1111-111111111111"));
    assert!(!x.ours);

    // An empty attribute is no value, and AuthorID is not mistaken for Author.
    let x = parse_xmp(r#"<rdf:Description vrc:WorldID="" vrc:AuthorID="usr_x"/>"#);
    assert_eq!((x.world_id, x.author_id.as_deref(), x.author), (None, Some("usr_x"), None));
}

#[test]
fn parse_xmp_prefers_elements_over_attributes() {
    let xml = r#"<rdf:Description vrc:WorldID="wrld_attr" vrc:WorldDisplayName="Attr World"
        xmp:CreateDate="2020-02-02T02:02:02+09:00"><vrc:WorldID>wrld_element</vrc:WorldID>
        <xmp:CreateDate>2026-01-10T20:30:00+09:00</xmp:CreateDate></rdf:Description>"#;
    let x = parse_xmp(xml);
    assert_eq!(x.world_id.as_deref(), Some("wrld_element"));
    assert_eq!(x.create_date, Some(dt(2026, 1, 10, 20, 30, 0)));
    // A field with no element still comes from its attribute.
    assert_eq!(x.world_name.as_deref(), Some("Attr World"));
}

#[test]
fn parse_xmp_empty_and_missing() {
    let x = parse_xmp("<vrc:WorldDisplayName></vrc:WorldDisplayName><vrc:WorldID> </vrc:WorldID>");
    assert_eq!(x.world_name, None);
    assert_eq!(x.world_id, None);
    assert_eq!(parse_xmp("<nothing/>"), Default::default());
    assert_eq!(parse_xmp("<xmp:CreateDate>garbage</xmp:CreateDate>").create_date, None);
}

#[test]
fn read_png_info_finds_dimensions_and_xmp() {
    let png = png_with_itxt(7, 5, "XML:com.adobe.xmp", XMP);
    let info = read_png_info(Cursor::new(&png)).unwrap().unwrap();
    assert_eq!((info.width, info.height), (7, 5));
    assert_eq!(info.xmp.as_deref(), Some(XMP));

    let other = png_with_itxt(3, 2, "Comment", "hello");
    let info = read_png_info(Cursor::new(&other)).unwrap().unwrap();
    assert_eq!((info.width, info.height), (3, 2));
    assert_eq!(info.xmp, None);

    assert!(read_png_info(Cursor::new(b"not a png at all".to_vec())).unwrap().is_none());
    assert!(read_png_info(Cursor::new(Vec::new())).unwrap().is_none());
}

#[test]
fn read_png_info_stops_before_pixel_data() {
    // Truncated right after IHDR + iTXt: no IDAT needed to answer.
    let png = png_with_itxt(4, 4, "XML:com.adobe.xmp", XMP);
    let cut = 8 + 25 + itxt_chunk("XML:com.adobe.xmp", XMP).len();
    let info = read_png_info(Cursor::new(&png[..cut])).unwrap().unwrap();
    assert_eq!((info.width, info.height), (4, 4));
    assert!(info.xmp.is_some());
}

#[test]
fn read_meta_priority() {
    let dir = tempfile::tempdir().unwrap();
    let mtime = dt(2020, 1, 1, 0, 0, 0);

    // XMP beats the file name when the two agree (saving takes a few seconds).
    let p = dir.path().join("VRChat_2026-01-10_20-29-58.678_100x50.png");
    std::fs::write(&p, png_with_itxt(100, 50, "XML:com.adobe.xmp", XMP)).unwrap();
    let m = read_meta(&p, mtime, true);
    assert_eq!(m.time_source, TimeSource::Xmp);
    assert_eq!(m.taken_at, dt(2026, 1, 10, 20, 30, 0));
    assert_eq!((m.width, m.height), (Some(100), Some(50)));
    assert_eq!(m.world_name.as_deref(), Some("Sample & World"));
    assert!(m.author_id.is_some());

    // Without reading content only the file name counts.
    let m = read_meta(&p, mtime, false);
    assert_eq!(m.time_source, TimeSource::Name);
    assert_eq!(m.taken_at, dt(2026, 1, 10, 20, 29, 58));
    assert_eq!(m.width, None);
    assert_eq!(m.world_id, None);

    // When they disagree (a saved print: the XMP has its creator's date), the file name wins.
    // The XMP's other values are still read.
    let far = dir.path().join("VRChat_2022-01-02_03-04-05.678_100x50.png");
    std::fs::write(&far, png_with_itxt(100, 50, "XML:com.adobe.xmp", XMP)).unwrap();
    let m = read_meta(&far, mtime, true);
    assert_eq!(m.time_source, TimeSource::Name);
    assert_eq!(m.taken_at, dt(2022, 1, 2, 3, 4, 5));
    assert_eq!(m.world_name.as_deref(), Some("Sample & World"));
    assert_eq!((m.width, m.height), (Some(100), Some(50)));

    // Plain PNG with a plain name falls back to mtime.
    let p2 = dir.path().join("plain.png");
    std::fs::write(&p2, png_with_itxt(2, 2, "Comment", "x")).unwrap();
    let m = read_meta(&p2, mtime, true);
    assert_eq!(m.time_source, TimeSource::Mtime);
    assert_eq!(m.taken_at, mtime);
    assert_eq!(m.time_source.as_str(), "mtime");

    // Unreadable file never fails.
    let m = read_meta(&dir.path().join("missing.png"), mtime, true);
    assert_eq!(m.taken_at, mtime);
    assert_eq!(m.width, None);
}

#[test]
fn choose_time_trusts_the_name_when_xmp_is_far_off() {
    let mtime = dt(2020, 1, 1, 0, 0, 0);
    let name = dt(2026, 11, 23, 0, 3, 28);
    // A camera photo: the XMP is written a moment after the name's time.
    let close = dt(2026, 11, 23, 0, 3, 30);
    assert_eq!(choose_time(Some(close), Some(name), mtime), (close, TimeSource::Xmp));
    // 5 minutes apart is still the same shot; more than that is not.
    let edge = dt(2026, 11, 23, 0, 8, 28);
    assert_eq!(choose_time(Some(edge), Some(name), mtime), (edge, TimeSource::Xmp));
    let past = dt(2026, 11, 22, 23, 58, 27);
    assert_eq!(choose_time(Some(past), Some(name), mtime), (name, TimeSource::Name));
    // A saved print whose XMP date was written 9 hours off.
    let print = dt(2026, 11, 23, 9, 2, 58);
    assert_eq!(choose_time(Some(print), Some(name), mtime), (name, TimeSource::Name));
    // With only one of them, that one is used.
    assert_eq!(choose_time(Some(print), None, mtime), (print, TimeSource::Xmp));
    assert_eq!(choose_time(None, Some(name), mtime), (name, TimeSource::Name));
    assert_eq!(choose_time(None, None, mtime), (mtime, TimeSource::Mtime));
}

#[test]
fn read_meta_jpeg_dimensions() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("a.JPG");
    image::RgbImage::new(30, 20).save(&p).unwrap();
    let m = read_meta(&p, dt(2020, 1, 1, 0, 0, 0), true);
    assert_eq!((m.width, m.height), (Some(30), Some(20)));
}

#[test]
fn read_meta_jpeg_uses_xmp_date_and_world() {
    let dir = tempfile::tempdir().unwrap();
    let mut jpg = Vec::new();
    image::RgbImage::new(30, 20)
        .write_to(&mut Cursor::new(&mut jpg), image::ImageFormat::Jpeg)
        .unwrap();
    let mut payload = b"http://ns.adobe.com/xap/1.0/\0".to_vec();
    payload.extend_from_slice(XMP.as_bytes());
    let mut seg = vec![0xFF, 0xE1];
    seg.extend_from_slice(&((payload.len() + 2) as u16).to_be_bytes());
    seg.extend_from_slice(&payload);
    let with_xmp = [&jpg[..2], &seg[..], &jpg[2..]].concat();

    let p = dir.path().join("VRChat_2026-01-10_20-29-58.678_30x20.jpg");
    std::fs::write(&p, &with_xmp).unwrap();
    let m = read_meta(&p, dt(2020, 1, 1, 0, 0, 0), true);
    assert_eq!(m.time_source, TimeSource::Xmp);
    assert_eq!(m.taken_at, dt(2026, 1, 10, 20, 30, 0));
    assert_eq!((m.width, m.height), (Some(30), Some(20)));
    assert_eq!(m.world_name.as_deref(), Some("Sample & World"));
    assert_eq!(m.world_id.as_deref(), Some("wrld_11111111-1111-1111-1111-111111111111"));
    assert!(m.author_id.is_some());

    // Without reading content, only the name counts; a JPEG without XMP is unchanged.
    assert_eq!(read_meta(&p, dt(2020, 1, 1, 0, 0, 0), false).time_source, TimeSource::Name);
    let plain = dir.path().join("VRChat_2022-01-02_03-04-05.678_a.jpg");
    std::fs::write(&plain, &jpg).unwrap();
    let m = read_meta(&plain, dt(2020, 1, 1, 0, 0, 0), true);
    assert_eq!((m.time_source, m.world_id), (TimeSource::Name, None));
}
