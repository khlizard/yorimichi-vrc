//! Photo metadata: capture time (XMP, file name, mtime), dimensions and VRChat XMP fields.

use std::fs::File;
use std::io::{self, BufReader, Read};
use std::path::Path;
use std::sync::OnceLock;

use chrono::{NaiveDate, NaiveDateTime};
use regex::Regex;

/// Cap for a single text chunk we are willing to read into memory.
const MAX_TEXT_CHUNK: u32 = 1024 * 1024;
pub(crate) const PNG_SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];

pub(crate) const XMP_KEYWORD: &str = "XML:com.adobe.xmp";
/// APP1 prefix of a JPEG XMP packet, and of the ExtendedXMP continuation.
pub(crate) const JPEG_XMP_SIG: &[u8] = b"http://ns.adobe.com/xap/1.0/\0";
pub(crate) const JPEG_XMP_EXT_SIG: &[u8] = b"http://ns.adobe.com/xmp/extension/\0";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeSource {
    Xmp,
    Name,
    Mtime,
}

impl TimeSource {
    pub fn as_str(self) -> &'static str {
        match self {
            TimeSource::Xmp => "xmp",
            TimeSource::Name => "name",
            TimeSource::Mtime => "mtime",
        }
    }
}

fn build(y: &str, mo: &str, d: &str, h: &str, mi: &str, s: &str) -> Option<NaiveDateTime> {
    NaiveDate::from_ymd_opt(y.parse().ok()?, mo.parse().ok()?, d.parse().ok()?)?
        .and_hms_opt(h.parse().ok()?, mi.parse().ok()?, s.parse().ok()?)
}

fn re(cell: &'static OnceLock<Regex>, pattern: &str) -> &'static Regex {
    cell.get_or_init(|| Regex::new(pattern).expect("valid regex"))
}

/// Capture time written in a file name (VRChat, Android-style `yyyymmdd_hhmmss`, ChatGPT exports).
pub fn time_from_name(file_name: &str) -> Option<NaiveDateTime> {
    static R1: OnceLock<Regex> = OnceLock::new();
    static R2: OnceLock<Regex> = OnceLock::new();
    static R3: OnceLock<Regex> = OnceLock::new();

    for c in re(&R1, r"(\d{4})-(\d\d)-(\d\d)[ _T](\d\d)[-_:.](\d\d)[-_:.](\d\d)").captures_iter(file_name) {
        if let Some(t) = build(&c[1], &c[2], &c[3], &c[4], &c[5], &c[6]) {
            return Some(t);
        }
    }
    // The regex crate has no look-around, so the digit boundary is checked by hand.
    let bytes = file_name.as_bytes();
    for c in re(&R2, r"(20\d{2})(\d\d)(\d\d)[_-](\d\d)(\d\d)(\d\d)").captures_iter(file_name) {
        let m = c.get(0).expect("whole match");
        let before_digit = m.start() > 0 && bytes[m.start() - 1].is_ascii_digit();
        let after_digit = bytes.get(m.end()).is_some_and(u8::is_ascii_digit);
        if before_digit || after_digit {
            continue;
        }
        if let Some(t) = build(&c[1], &c[2], &c[3], &c[4], &c[5], &c[6]) {
            return Some(t);
        }
    }
    for c in re(&R3, r"ChatGPT Image (\d{4})年(\d{1,2})月(\d{1,2})日 (\d{1,2})_(\d{1,2})_(\d{1,2})")
        .captures_iter(file_name)
    {
        if let Some(t) = build(&c[1], &c[2], &c[3], &c[4], &c[5], &c[6]) {
            return Some(t);
        }
    }
    None
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct XmpInfo {
    pub create_date: Option<NaiveDateTime>,
    pub world_id: Option<String>,
    pub world_name: Option<String>,
    pub author_id: Option<String>,
    pub author: Option<String>,
    /// A `vrclogger:Source` element is present, i.e. this tool wrote the packet.
    pub ours: bool,
}

fn unescape(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

/// The XMP fields read by [`parse_xmp`].
const XMP_FIELDS: [&str; 5] = ["CreateDate", "WorldID", "WorldDisplayName", "AuthorID", "Author"];

/// How one field is written: as an element (`<vrc:WorldID>wrld_..</vrc:WorldID>`) or, in a file
/// saved again by an image editor, as an attribute (`vrc:WorldID="wrld_.."` on `rdf:Description`).
struct FieldPatterns {
    tag: &'static str,
    element: Regex,
    attribute: Regex,
}

/// Compiled once: [`parse_xmp`] runs for every photo scanned.
fn field_patterns() -> &'static [FieldPatterns] {
    static PATTERNS: OnceLock<Vec<FieldPatterns>> = OnceLock::new();
    PATTERNS.get_or_init(|| {
        XMP_FIELDS
            .iter()
            .map(|&tag| FieldPatterns {
                tag,
                element: Regex::new(&format!(r"<(?:[\w.-]+:)?{tag}(?:\s[^>]*)?>([^<]*)</")).expect("valid pattern"),
                attribute: Regex::new(&format!(r#"\s(?:[\w.-]+:)?{tag}\s*=\s*(?:"([^"]*)"|'([^']*)')"#))
                    .expect("valid pattern"),
            })
            .collect()
    })
}

/// Text of the first `<prefix:tag>text</...>` element, else the value of the first `prefix:tag="..."`
/// attribute; empty text is `None`. An element wins over an attribute.
fn element_text(xml: &str, tag: &str) -> Option<String> {
    let f = field_patterns().iter().find(|f| f.tag == tag)?;
    let clean = |s: &str| Some(unescape(s.trim())).filter(|t| !t.is_empty());
    f.element.captures(xml).and_then(|m| clean(&m[1])).or_else(|| {
        let m = f.attribute.captures(xml)?;
        clean(m.get(1).or_else(|| m.get(2))?.as_str())
    })
}

pub fn parse_xmp(xml: &str) -> XmpInfo {
    // Wall-clock part only: the DB stores local time and the written offset is the local one.
    let create_date = element_text(xml, "CreateDate")
        .and_then(|s| NaiveDateTime::parse_from_str(s.get(..19)?, "%Y-%m-%dT%H:%M:%S").ok());
    XmpInfo {
        create_date,
        world_id: element_text(xml, "WorldID"),
        world_name: element_text(xml, "WorldDisplayName"),
        author_id: element_text(xml, "AuthorID"),
        author: element_text(xml, "Author"),
        ours: xml.contains("<vrclogger:Source>"),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PngInfo {
    pub width: u32,
    pub height: u32,
    pub xmp: Option<String>,
}

fn be_u32(b: &[u8]) -> u32 {
    u32::from_be_bytes([b[0], b[1], b[2], b[3]])
}

fn parse_itxt(data: &[u8]) -> Option<String> {
    let kw_end = data.iter().position(|&b| b == 0)?;
    if &data[..kw_end] != XMP_KEYWORD.as_bytes() {
        return None;
    }
    let rest = data.get(kw_end + 1..)?;
    let (&comp_flag, rest) = rest.split_first()?;
    let rest = rest.get(1..)?; // compression method
    if comp_flag != 0 {
        return None;
    }
    let lang_end = rest.iter().position(|&b| b == 0)?;
    let rest = &rest[lang_end + 1..];
    let tr_end = rest.iter().position(|&b| b == 0)?;
    Some(String::from_utf8_lossy(&rest[tr_end + 1..]).into_owned())
}

/// Reads dimensions and XMP from a PNG without touching pixel data (stops at the first `IDAT`).
/// `Ok(None)` if the stream is not a PNG.
pub fn read_png_info(mut reader: impl Read) -> io::Result<Option<PngInfo>> {
    let mut sig = [0u8; 8];
    match reader.read_exact(&mut sig) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e),
    }
    if sig != PNG_SIGNATURE {
        return Ok(None);
    }
    let mut dims: Option<(u32, u32)> = None;
    let mut xmp = None;
    loop {
        let mut head = [0u8; 8];
        if reader.read_exact(&mut head).is_err() {
            break;
        }
        let len = be_u32(&head[..4]);
        match &head[4..] {
            b"IDAT" | b"IEND" => break,
            b"IHDR" if (8..=MAX_TEXT_CHUNK).contains(&len) => {
                let mut buf = Vec::new();
                (&mut reader).take(u64::from(len) + 4).read_to_end(&mut buf)?;
                if buf.len() < 8 {
                    break;
                }
                dims = Some((be_u32(&buf[..4]), be_u32(&buf[4..8])));
            }
            b"iTXt" if len <= MAX_TEXT_CHUNK && xmp.is_none() => {
                let mut buf = Vec::new();
                (&mut reader).take(u64::from(len) + 4).read_to_end(&mut buf)?;
                if buf.len() < len as usize {
                    break;
                }
                xmp = parse_itxt(&buf[..len as usize]);
            }
            _ => {
                // Skip data + CRC without seeking so any reader works.
                io::copy(&mut (&mut reader).take(u64::from(len) + 4), &mut io::sink())?;
            }
        }
    }
    Ok(dims.map(|(width, height)| PngInfo { width, height, xmp }))
}

/// Payloads of all APP1 segments before the scan data (`Ok(None)` if not a JPEG).
/// A truncated or malformed stream just ends the walk.
pub(crate) fn jpeg_app1_payloads(mut reader: impl Read) -> io::Result<Option<Vec<Vec<u8>>>> {
    let mut soi = [0u8; 2];
    match reader.read_exact(&mut soi) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e),
    }
    if soi != [0xFF, 0xD8] {
        return Ok(None);
    }
    let mut out = Vec::new();
    match walk_jpeg_segments(&mut reader, &mut out) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => {}
        Err(e) => return Err(e),
    }
    Ok(Some(out))
}

fn walk_jpeg_segments(reader: &mut impl Read, out: &mut Vec<Vec<u8>>) -> io::Result<()> {
    loop {
        let mut b = [0u8; 1];
        reader.read_exact(&mut b)?;
        if b[0] != 0xFF {
            return Ok(());
        }
        while b[0] == 0xFF {
            reader.read_exact(&mut b)?; // fill bytes
        }
        match b[0] {
            0x00 | 0x01 | 0xD0..=0xD8 => continue, // no length field
            0xD9 | 0xDA => return Ok(()),          // EOI / start of scan
            _ => {}
        }
        let mut len = [0u8; 2];
        reader.read_exact(&mut len)?;
        let Some(n) = usize::from(u16::from_be_bytes(len)).checked_sub(2) else {
            return Ok(());
        };
        if b[0] == 0xE1 {
            let mut buf = Vec::with_capacity(n);
            reader.take(n as u64).read_to_end(&mut buf)?;
            if buf.len() < n {
                return Ok(());
            }
            out.push(buf);
        } else {
            io::copy(&mut reader.take(n as u64), &mut io::sink())?;
        }
    }
}

/// XMP packet of a JPEG (APP1 `http://ns.adobe.com/xap/1.0/`), read without touching pixel data.
/// ExtendedXMP continuation segments are ignored.
pub fn read_jpeg_xmp(reader: impl Read) -> io::Result<Option<String>> {
    Ok(jpeg_app1_payloads(reader)?.and_then(|segs| {
        segs.iter()
            .find_map(|s| s.strip_prefix(JPEG_XMP_SIG))
            .map(|x| String::from_utf8_lossy(x).into_owned())
    }))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhotoMeta {
    pub taken_at: NaiveDateTime,
    pub time_source: TimeSource,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub world_id: Option<String>,
    pub world_name: Option<String>,
    pub author_id: Option<String>,
}

/// Never fails: I/O errors only mean less information. With `read_content = false` the file
/// is not opened at all (cloud placeholders would be downloaded).
pub fn read_meta(path: &Path, mtime: NaiveDateTime, read_content: bool) -> PhotoMeta {
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    let mut width = None;
    let mut height = None;
    let mut xmp = XmpInfo::default();

    if read_content {
        if ext == "png" {
            let info = File::open(path).and_then(|f| read_png_info(BufReader::new(f))).ok().flatten();
            if let Some(info) = info {
                width = Some(info.width);
                height = Some(info.height);
                if let Some(x) = &info.xmp {
                    xmp = parse_xmp(x);
                }
            }
        } else if matches!(ext.as_str(), "jpg" | "jpeg" | "webp") {
            if ext != "webp" {
                let x = File::open(path).and_then(|f| read_jpeg_xmp(BufReader::new(f))).ok().flatten();
                if let Some(x) = x {
                    xmp = parse_xmp(&x);
                }
            }
            let dims = image::ImageReader::open(path)
                .and_then(|r| r.with_guessed_format())
                .ok()
                .and_then(|r| r.into_dimensions().ok());
            if let Some((w, h)) = dims {
                width = Some(w);
                height = Some(h);
            }
        }
    }

    let name_time = path.file_name().and_then(|n| time_from_name(&n.to_string_lossy()));
    let (taken_at, time_source) = if let Some(t) = xmp.create_date {
        (t, TimeSource::Xmp)
    } else if let Some(t) = name_time {
        (t, TimeSource::Name)
    } else {
        (mtime, TimeSource::Mtime)
    };
    PhotoMeta {
        taken_at,
        time_source,
        width,
        height,
        world_id: xmp.world_id,
        world_name: xmp.world_name,
        author_id: xmp.author_id,
    }
}
