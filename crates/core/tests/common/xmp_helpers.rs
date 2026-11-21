//! Image-building helpers for the XMP writing tests (included with `#[path]`, not a test target).
#![allow(dead_code)]

use std::io::Cursor;

pub const USR: &str = "usr_11111111-1111-1111-1111-111111111111";
pub const WRLD: &str = "wrld_11111111-1111-1111-1111-111111111111";

pub fn gradient(w: u32, h: u32) -> image::RgbImage {
    image::RgbImage::from_fn(w, h, |x, y| image::Rgb([(x * 7) as u8, (y * 5) as u8, (x + y) as u8]))
}

pub fn png_bytes(w: u32, h: u32) -> Vec<u8> {
    let mut buf = Vec::new();
    gradient(w, h).write_to(&mut Cursor::new(&mut buf), image::ImageFormat::Png).unwrap();
    buf
}

pub fn jpeg_bytes(w: u32, h: u32) -> Vec<u8> {
    let mut buf = Vec::new();
    gradient(w, h).write_to(&mut Cursor::new(&mut buf), image::ImageFormat::Jpeg).unwrap();
    buf
}

pub fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &b in data {
        crc ^= u32::from(b);
        for _ in 0..8 {
            crc = if crc & 1 != 0 { (crc >> 1) ^ 0xEDB8_8320 } else { crc >> 1 };
        }
    }
    !crc
}

/// Raw PNG chunk with a correct CRC.
pub fn chunk(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let mut body = kind.to_vec();
    body.extend_from_slice(data);
    let mut out = (data.len() as u32).to_be_bytes().to_vec();
    out.extend_from_slice(&body);
    out.extend_from_slice(&crc32(&body).to_be_bytes());
    out
}

pub fn itxt(keyword: &str, compressed: bool, text: &str) -> Vec<u8> {
    let mut d = keyword.as_bytes().to_vec();
    d.extend_from_slice(&[0, u8::from(compressed), 0, 0, 0]);
    d.extend_from_slice(text.as_bytes());
    chunk(b"iTXt", &d)
}

/// `(kind, start, end)` of every chunk.
pub fn png_layout(png: &[u8]) -> Vec<([u8; 4], usize, usize)> {
    let mut out = Vec::new();
    let mut pos = 8;
    while pos + 12 <= png.len() {
        let len = u32::from_be_bytes(png[pos..pos + 4].try_into().unwrap()) as usize;
        out.push((png[pos + 4..pos + 8].try_into().unwrap(), pos, pos + 12 + len));
        pos += 12 + len;
    }
    out
}

pub fn png_kinds(png: &[u8]) -> Vec<String> {
    png_layout(png).iter().map(|(k, ..)| String::from_utf8_lossy(k).into_owned()).collect()
}

/// Inserts `chunk` before the chunk at `index`.
pub fn png_insert_at(png: &[u8], index: usize, chunk: &[u8]) -> Vec<u8> {
    let at = png_layout(png)[index].1;
    [&png[..at], chunk, &png[at..]].concat()
}

/// PNG with an extra chunk right after IHDR.
pub fn png_with_chunk(w: u32, h: u32, chunk: &[u8]) -> Vec<u8> {
    png_insert_at(&png_bytes(w, h), 1, chunk)
}

/// `(marker, payload)` of every JPEG segment before SOS.
pub fn jpeg_segments(j: &[u8]) -> Vec<(u8, Vec<u8>)> {
    let mut out = Vec::new();
    let mut pos = 2;
    while pos + 4 <= j.len() && j[pos] == 0xFF && j[pos + 1] != 0xDA {
        let len = u16::from_be_bytes([j[pos + 2], j[pos + 3]]) as usize;
        out.push((j[pos + 1], j[pos + 4..pos + 2 + len].to_vec()));
        pos += 2 + len;
    }
    out
}

pub fn jpeg_app1(payload: &[u8]) -> Vec<u8> {
    let mut s = vec![0xFF, 0xE1];
    s.extend_from_slice(&((payload.len() + 2) as u16).to_be_bytes());
    s.extend_from_slice(payload);
    s
}

/// Inserts `segment` right after SOI.
pub fn jpeg_with_segment_after_soi(j: &[u8], segment: &[u8]) -> Vec<u8> {
    [&j[..2], segment, &j[2..]].concat()
}

/// XMP exactly in the shape VRChat writes (elements, no xpacket).
pub fn vrchat_xmp(world_name: &str) -> String {
    format!(
        "<x:xmpmeta xmlns:x=\"adobe:ns:meta/\"><rdf:RDF><rdf:Description>\
         <xmp:CreatorTool>VRChat</xmp:CreatorTool><xmp:Author>Someone</xmp:Author>\
         <xmp:CreateDate>2026-01-10T20:40:00.1234567+09:00</xmp:CreateDate></rdf:Description>\
         <rdf:Description><vrc:WorldID>{WRLD}</vrc:WorldID>\
         <vrc:WorldDisplayName>{world_name}</vrc:WorldDisplayName><vrc:AuthorID>{USR}</vrc:AuthorID>\
         </rdf:Description></rdf:RDF></x:xmpmeta>"
    )
}

/// Photoshop style: attributes inside an xpacket.
pub fn photoshop_xmp() -> String {
    format!(
        "<?xpacket begin=\"\u{feff}\" id=\"W5M0MpCehiHzreSzNTczkc9d\"?><x:xmpmeta xmlns:x=\"adobe:ns:meta/\">\
         <rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\">\
         <rdf:Description xmlns:vrc=\"http://ns.vrchat.com/vrc/1.0/\" vrc:WorldID=\"{WRLD}\" \
         vrc:WorldDisplayName=\"W\"/></rdf:RDF></x:xmpmeta><?xpacket end=\"w\"?>"
    )
}
