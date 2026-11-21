//! Writing log-derived metadata into photos: the single exception to "photos are read-only".
//!
//! The approach is deliberately conservative:
//! * one metadata segment (PNG `iTXt` chunk / JPEG APP1 segment) is inserted at the byte level;
//!   pixel data is never decoded or re-encoded and no other byte changes;
//! * files that already carry any XMP are never touched;
//! * the result is verified in memory (stripping it must give back the exact original), written
//!   to a temp file next to the original, re-read, and only then renamed over the original;
//! * the file's timestamps are restored, and the insertion is reversible ([`strip_our_xmp`]).

use std::fs::{self, File, FileTimes, OpenOptions};
use std::io::{self, BufReader, Cursor, Read, Write};
use std::ops::Range;
use std::path::Path;

use chrono::{NaiveDateTime, Timelike};

use super::meta::{
    jpeg_app1_payloads, parse_xmp, read_jpeg_xmp, read_png_info, JPEG_XMP_EXT_SIG, JPEG_XMP_SIG, PNG_SIGNATURE,
    XMP_KEYWORD,
};
use super::scan::is_online_only;
use crate::{Error, Result};

/// Namespace of our provenance block.
const OUR_NS: &str = "urn:vrclogger:meta:1.0";
/// Largest APP1 payload (the 2-byte length field counts itself).
const MAX_JPEG_PAYLOAD: usize = 65_533;
/// Text chunks bigger than this are not read into memory when looking for XMP.
const MAX_TEXT_CHUNK: usize = 16 * 1024 * 1024;
const EXIF_SIG: &[u8] = b"Exif\0\0";

/// Everything that goes into the packet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XmpFields {
    pub creator_tool: Option<String>,
    pub author: Option<String>,
    pub author_id: Option<String>,
    /// Capture time as local wall-clock time.
    pub create_date: NaiveDateTime,
    pub utc_offset_minutes: i32,
    pub world_id: String,
    pub world_name: String,
    pub written_at: NaiveDateTime,
}

/// Where XMP was found in a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XmpPresence {
    None,
    /// Written by this tool (carries the `vrclogger:Source` marker).
    Ours,
    /// Any other XMP, including compressed or extended packets we cannot read.
    Other,
}

// ---- building ----------------------------------------------------------------------------

fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            // Control characters are not allowed in XML 1.0 at all.
            c if (c as u32) < 0x20 && !matches!(c, '\t' | '\n' | '\r') => {}
            c => out.push(c),
        }
    }
    out
}

fn fmt_date(dt: NaiveDateTime, offset_minutes: i32) -> String {
    let sign = if offset_minutes < 0 { '-' } else { '+' };
    let a = offset_minutes.unsigned_abs();
    format!("{}{sign}{:02}:{:02}", dt.format("%Y-%m-%dT%H:%M:%S"), a / 60, a % 60)
}

/// The packet, shaped like the one VRChat writes: one `rdf:Description` per namespace, values as
/// elements, no `xpacket` wrapper, plus our own provenance block.
pub fn build_xmp(f: &XmpFields) -> String {
    let created = fmt_date(f.create_date, f.utc_offset_minutes);
    let mut x = String::new();
    x.push_str("<x:xmpmeta xmlns:x=\"adobe:ns:meta/\">\n");
    x.push_str("  <rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\" xmlns:xmp=\"http://ns.adobe.com/xap/1.0/\">\n");
    x.push_str("    <rdf:Description>\n");
    if let Some(t) = &f.creator_tool {
        x.push_str(&format!("      <xmp:CreatorTool>{}</xmp:CreatorTool>\n", escape(t)));
    }
    if let Some(a) = &f.author {
        x.push_str(&format!("      <xmp:Author>{}</xmp:Author>\n", escape(a)));
    }
    x.push_str(&format!("      <xmp:CreateDate>{created}</xmp:CreateDate>\n"));
    x.push_str(&format!("      <xmp:ModifyDate>{created}</xmp:ModifyDate>\n"));
    x.push_str("    </rdf:Description>\n");
    x.push_str("    <rdf:Description xmlns:tiff=\"http://ns.adobe.com/tiff/1.0/\">\n");
    x.push_str(&format!("      <tiff:DateTime>{created}</tiff:DateTime>\n"));
    x.push_str("    </rdf:Description>\n");
    x.push_str("    <rdf:Description xmlns:vrc=\"http://ns.vrchat.com/vrc/1.0/\">\n");
    x.push_str(&format!("      <vrc:WorldID>{}</vrc:WorldID>\n", escape(&f.world_id)));
    x.push_str(&format!("      <vrc:WorldDisplayName>{}</vrc:WorldDisplayName>\n", escape(&f.world_name)));
    if let Some(a) = &f.author_id {
        x.push_str(&format!("      <vrc:AuthorID>{}</vrc:AuthorID>\n", escape(a)));
    }
    x.push_str("    </rdf:Description>\n");
    x.push_str(&format!("    <rdf:Description xmlns:vrclogger=\"{OUR_NS}\">\n"));
    x.push_str("      <vrclogger:Source>log</vrclogger:Source>\n");
    x.push_str(&format!(
        "      <vrclogger:WrittenAt>{}</vrclogger:WrittenAt>\n",
        fmt_date(f.written_at, f.utc_offset_minutes)
    ));
    x.push_str("    </rdf:Description>\n");
    x.push_str("  </rdf:RDF>\n");
    x.push_str("</x:xmpmeta>");
    x
}

// ---- CRC-32 (PNG) ------------------------------------------------------------------------

const CRC_TABLE: [u32; 256] = {
    let mut t = [0u32; 256];
    let mut i = 0;
    while i < 256 {
        let mut c = i as u32;
        let mut k = 0;
        while k < 8 {
            c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
            k += 1;
        }
        t[i] = c;
        i += 1;
    }
    t
};

fn crc32(data: &[u8]) -> u32 {
    let mut c = 0xFFFF_FFFFu32;
    for &b in data {
        c = CRC_TABLE[((c ^ u32::from(b)) & 0xFF) as usize] ^ (c >> 8);
    }
    !c
}

// ---- format helpers ----------------------------------------------------------------------

fn invalid(msg: &str) -> Error {
    Error::Invalid(msg.to_string())
}

fn is_png(data: &[u8]) -> bool {
    data.starts_with(&PNG_SIGNATURE)
}

fn is_jpeg(data: &[u8]) -> bool {
    data.starts_with(&[0xFF, 0xD8])
}

fn be_u32(b: &[u8]) -> u32 {
    u32::from_be_bytes([b[0], b[1], b[2], b[3]])
}

struct PngChunk {
    start: usize,
    end: usize,
    kind: [u8; 4],
    data: Range<usize>,
}

/// Chunks up to and including `IEND`. Strict: the first chunk must be `IHDR` and no chunk may
/// run past the end of the data.
fn png_chunks(data: &[u8]) -> Result<Vec<PngChunk>> {
    let mut chunks = Vec::new();
    let mut pos = PNG_SIGNATURE.len();
    loop {
        if pos + 12 > data.len() {
            return Err(invalid("malformed PNG: truncated before IEND"));
        }
        let len = be_u32(&data[pos..]) as usize;
        let end = (pos + 12)
            .checked_add(len)
            .filter(|&e| e <= data.len())
            .ok_or_else(|| invalid("malformed PNG: chunk runs past the end of the file"))?;
        let kind = [data[pos + 4], data[pos + 5], data[pos + 6], data[pos + 7]];
        if chunks.is_empty() && (&kind != b"IHDR" || len != 13) {
            return Err(invalid("malformed PNG: IHDR missing"));
        }
        chunks.push(PngChunk { start: pos, end, kind, data: pos + 8..pos + 8 + len });
        if &kind == b"IEND" {
            return Ok(chunks);
        }
        pos = end;
    }
}

enum XmpText {
    Plain(String),
    /// Compressed: present but unreadable for us.
    Compressed,
}

/// XMP held by an `iTXt` / `tEXt` / `zTXt` chunk body, `None` for any other keyword.
fn text_chunk_xmp(kind: &[u8], body: &[u8]) -> Option<XmpText> {
    if !matches!(kind, b"iTXt" | b"tEXt" | b"zTXt") {
        return None;
    }
    let kw_end = body.iter().position(|&b| b == 0)?;
    if &body[..kw_end] != XMP_KEYWORD.as_bytes() {
        return None;
    }
    let rest = &body[kw_end + 1..];
    match kind {
        b"zTXt" => Some(XmpText::Compressed),
        b"tEXt" => Some(XmpText::Plain(String::from_utf8_lossy(rest).into_owned())),
        _ => {
            let (&flag, rest) = rest.split_first()?;
            if flag != 0 {
                return Some(XmpText::Compressed);
            }
            let rest = rest.get(1..)?; // compression method
            let lang_end = rest.iter().position(|&b| b == 0)?;
            let rest = &rest[lang_end + 1..];
            let tr_end = rest.iter().position(|&b| b == 0)?;
            Some(XmpText::Plain(String::from_utf8_lossy(&rest[tr_end + 1..]).into_owned()))
        }
    }
}

struct JpegSeg {
    start: usize,
    end: usize,
    marker: u8,
}

impl JpegSeg {
    fn payload<'a>(&self, data: &'a [u8]) -> &'a [u8] {
        data.get(self.start + 4..self.end).unwrap_or(&[])
    }
}

/// Segments between SOI and SOS. Strict: anything unexpected is an error.
fn jpeg_segments(data: &[u8]) -> Result<Vec<JpegSeg>> {
    let bad = || invalid("malformed JPEG");
    let mut segs = Vec::new();
    let mut pos = 2;
    loop {
        if *data.get(pos).ok_or_else(bad)? != 0xFF {
            return Err(bad());
        }
        let mut m = pos + 1;
        while *data.get(m).ok_or_else(bad)? == 0xFF {
            m += 1; // fill bytes
        }
        let marker = data[m];
        match marker {
            0xDA => return Ok(segs),
            0x00 | 0xD8 | 0xD9 => return Err(bad()),
            0x01 | 0xD0..=0xD7 => {
                segs.push(JpegSeg { start: m - 1, end: m + 1, marker });
                pos = m + 1;
            }
            _ => {
                let len = data.get(m + 1..m + 3).ok_or_else(bad)?;
                let len = usize::from(u16::from_be_bytes([len[0], len[1]]));
                let end = m + 1 + len;
                if len < 2 || end > data.len() {
                    return Err(bad());
                }
                segs.push(JpegSeg { start: m - 1, end, marker });
                pos = end;
            }
        }
    }
}

// ---- in-memory insert / strip -------------------------------------------------------------

/// Inserts `xmp` as one new `iTXt` chunk (PNG, right after `IHDR`) or APP1 segment (JPEG, after
/// the leading APP0 / EXIF segments). Nothing else changes.
pub fn insert_xmp(data: &[u8], xmp: &str) -> Result<Vec<u8>> {
    if is_png(data) {
        let chunks = png_chunks(data)?;
        let at = chunks[0].end;
        let mut body = b"iTXt".to_vec();
        body.extend_from_slice(XMP_KEYWORD.as_bytes());
        body.extend_from_slice(&[0, 0, 0, 0, 0]); // nul, uncompressed, method, empty language, empty translated keyword
        body.extend_from_slice(xmp.as_bytes());
        let data_len = u32::try_from(body.len() - 4).map_err(|_| invalid("XMP too large for a PNG chunk"))?;
        let mut chunk = data_len.to_be_bytes().to_vec();
        chunk.extend_from_slice(&body);
        chunk.extend_from_slice(&crc32(&body).to_be_bytes());
        Ok(splice(data, at, at, &chunk))
    } else if is_jpeg(data) {
        let segs = jpeg_segments(data)?;
        let payload_len = JPEG_XMP_SIG.len() + xmp.len();
        if payload_len > MAX_JPEG_PAYLOAD {
            return Err(Error::Invalid(format!(
                "XMP is too large for a JPEG segment ({payload_len} > {MAX_JPEG_PAYLOAD} bytes)"
            )));
        }
        let mut at = 2;
        for s in &segs {
            let leading = s.marker == 0xE0 || (s.marker == 0xE1 && s.payload(data).starts_with(EXIF_SIG));
            if !leading {
                break;
            }
            at = s.end;
        }
        let mut seg = vec![0xFF, 0xE1];
        seg.extend_from_slice(&((payload_len + 2) as u16).to_be_bytes());
        seg.extend_from_slice(JPEG_XMP_SIG);
        seg.extend_from_slice(xmp.as_bytes());
        Ok(splice(data, at, at, &seg))
    } else {
        Err(invalid("unsupported format"))
    }
}

fn splice(data: &[u8], from: usize, to: usize, insert: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() - (to - from) + insert.len());
    out.extend_from_slice(&data[..from]);
    out.extend_from_slice(insert);
    out.extend_from_slice(&data[to..]);
    out
}

fn is_ours(text: &str) -> bool {
    parse_xmp(text).ours
}

/// Removes the chunk / segment(s) holding XMP with our `vrclogger:Source` marker.
/// `Ok(None)` if there is none; foreign XMP is never removed.
pub fn strip_our_xmp(data: &[u8]) -> Result<Option<Vec<u8>>> {
    let mut ranges: Vec<Range<usize>> = Vec::new();
    if is_png(data) {
        for c in png_chunks(data)? {
            if let Some(XmpText::Plain(t)) = text_chunk_xmp(&c.kind, &data[c.data.clone()]) {
                if is_ours(&t) {
                    ranges.push(c.start..c.end);
                }
            }
        }
    } else if is_jpeg(data) {
        for s in jpeg_segments(data)? {
            if s.marker != 0xE1 {
                continue;
            }
            if let Some(x) = s.payload(data).strip_prefix(JPEG_XMP_SIG) {
                if is_ours(&String::from_utf8_lossy(x)) {
                    ranges.push(s.start..s.end);
                }
            }
        }
    } else {
        return Err(invalid("unsupported format"));
    }
    if ranges.is_empty() {
        return Ok(None);
    }
    let mut out = Vec::with_capacity(data.len());
    let mut pos = 0;
    for r in ranges {
        out.extend_from_slice(&data[pos..r.start]);
        pos = r.end;
    }
    out.extend_from_slice(&data[pos..]);
    Ok(Some(out))
}

/// XMP text of an in-memory image, read by the same readers the scanner uses.
fn read_xmp_from_bytes(data: &[u8]) -> Option<String> {
    if is_png(data) {
        read_png_info(Cursor::new(data)).ok().flatten().and_then(|i| i.xmp)
    } else {
        read_jpeg_xmp(Cursor::new(data)).ok().flatten()
    }
}

// ---- presence detection --------------------------------------------------------------------

fn merge(found: &mut XmpPresence, text: Option<&str>) {
    let p = match text {
        Some(t) if is_ours(t) => XmpPresence::Ours,
        _ => XmpPresence::Other,
    };
    if *found != XmpPresence::Ours {
        *found = p;
    }
}

/// Detects any XMP in a PNG / JPEG without reading pixel data. Other formats report `None`.
pub fn xmp_presence(path: &Path) -> io::Result<XmpPresence> {
    let mut file = BufReader::new(File::open(path)?);
    let mut sig = [0u8; 8];
    let n = read_up_to(&mut file, &mut sig)?;
    if n == 8 && sig == PNG_SIGNATURE {
        png_presence(&mut file)
    } else if n >= 2 && sig[..2] == [0xFF, 0xD8] {
        drop(file);
        let payloads = jpeg_app1_payloads(BufReader::new(File::open(path)?))?.unwrap_or_default();
        let mut found = XmpPresence::None;
        for p in &payloads {
            if let Some(x) = p.strip_prefix(JPEG_XMP_SIG) {
                merge(&mut found, Some(&String::from_utf8_lossy(x)));
            } else if p.starts_with(JPEG_XMP_EXT_SIG) {
                merge(&mut found, None);
            }
        }
        Ok(found)
    } else {
        Ok(XmpPresence::None)
    }
}

fn read_up_to(r: &mut impl Read, buf: &mut [u8]) -> io::Result<usize> {
    let mut n = 0;
    while n < buf.len() {
        match r.read(&mut buf[n..])? {
            0 => break,
            k => n += k,
        }
    }
    Ok(n)
}

/// Walks every chunk (also after `IDAT`: some editors append text chunks), reading only text chunks.
fn png_presence(file: &mut BufReader<File>) -> io::Result<XmpPresence> {
    let mut found = XmpPresence::None;
    loop {
        let mut head = [0u8; 8];
        if read_up_to(file, &mut head)? < 8 {
            break;
        }
        let len = be_u32(&head) as usize;
        let kind = &head[4..];
        if kind == b"IEND" {
            break;
        }
        if matches!(kind, b"iTXt" | b"tEXt" | b"zTXt") {
            let n = len.min(MAX_TEXT_CHUNK);
            let mut body = vec![0u8; n];
            if read_up_to(file, &mut body)? < n {
                break;
            }
            let mut prefix = XMP_KEYWORD.as_bytes().to_vec();
            prefix.push(0);
            if len > n {
                if body.starts_with(&prefix) {
                    merge(&mut found, None);
                }
            } else {
                match text_chunk_xmp(kind, &body) {
                    Some(XmpText::Plain(t)) => merge(&mut found, Some(&t)),
                    Some(XmpText::Compressed) => merge(&mut found, None),
                    None => {}
                }
            }
            file.seek_relative((len - n) as i64 + 4)?;
        } else {
            file.seek_relative(len as i64 + 4)?;
        }
    }
    Ok(found)
}

// ---- file operations -----------------------------------------------------------------------

fn refuse_unwritable(md: &fs::Metadata) -> Result<()> {
    if !md.is_file() {
        return Err(invalid("not a regular file"));
    }
    if is_online_only(md) {
        return Err(invalid("the file is online-only; it would have to be downloaded first"));
    }
    if md.permissions().readonly() {
        return Err(invalid("the file is read-only"));
    }
    Ok(())
}

fn normalized(s: Option<&str>) -> Option<&str> {
    s.map(str::trim).filter(|s| !s.is_empty())
}

/// In-memory proof that `new` is `orig` plus exactly our packet, and that the scanner's reader
/// finds the intended values in it.
fn verify_inserted(orig: &[u8], new: &[u8], f: &XmpFields) -> Result<()> {
    let fail = |what: &str| Error::Invalid(format!("verification failed: {what}"));
    if strip_our_xmp(new)?.as_deref() != Some(orig) {
        return Err(fail("removing the packet does not restore the original bytes"));
    }
    let text = read_xmp_from_bytes(new).ok_or_else(|| fail("the packet cannot be read back"))?;
    let x = parse_xmp(&text);
    let ok = x.ours
        && x.world_id.as_deref() == normalized(Some(&f.world_id))
        && x.world_name.as_deref() == normalized(Some(&f.world_name))
        && x.author_id.as_deref() == normalized(f.author_id.as_deref())
        && x.create_date == f.create_date.with_nanosecond(0);
    if ok {
        Ok(())
    } else {
        Err(fail("the packet read back differs from what was meant to be written"))
    }
}

/// Atomically replaces `path` with `new` (temp file in the same directory, re-read, rename) and
/// restores the timestamps `before` had. On any failure before the rename the original is untouched.
fn replace_file(path: &Path, before: &fs::Metadata, new: &[u8]) -> Result<()> {
    let name = path.file_name().ok_or_else(|| invalid("path has no file name"))?;
    let tmp = path.with_file_name(format!(".{}.vrclogger-tmp", name.to_string_lossy()));

    let staged = (|| -> Result<()> {
        let _ = fs::remove_file(&tmp); // leftover of an interrupted run
        let mut f = OpenOptions::new().write(true).create_new(true).open(&tmp)?;
        f.write_all(new)?;
        f.flush()?;
        f.sync_all()?;
        drop(f);
        if fs::read(&tmp)? != new {
            return Err(invalid("verification failed: the temporary file differs from the intended bytes"));
        }
        let now = fs::metadata(path)?;
        if now.len() != before.len() || now.modified()? != before.modified()? {
            return Err(invalid("the file changed while it was being processed"));
        }
        fs::rename(&tmp, path)?;
        Ok(())
    })();
    if let Err(e) = staged {
        let _ = fs::remove_file(&tmp);
        return Err(e);
    }

    let mut times = FileTimes::new();
    if let Ok(t) = before.modified() {
        times = times.set_modified(t);
    }
    if let Ok(t) = before.accessed() {
        times = times.set_accessed(t);
    }
    #[cfg(windows)]
    if let Ok(t) = before.created() {
        use std::os::windows::fs::FileTimesExt;
        times = times.set_created(t);
    }
    OpenOptions::new()
        .write(true)
        .open(path)
        .and_then(|f| f.set_times(times))
        .map_err(|e| Error::Invalid(format!("the file was updated but its timestamps could not be restored: {e}")))
}

/// Inserts our XMP into the file. Refuses read-only, online-only and already-tagged files.
pub fn write_xmp_to_file(path: &Path, f: &XmpFields) -> Result<()> {
    let md = fs::metadata(path)?;
    refuse_unwritable(&md)?;
    if xmp_presence(path)? != XmpPresence::None {
        return Err(invalid("the file already contains XMP metadata"));
    }
    let orig = fs::read(path)?;
    let new = insert_xmp(&orig, &build_xmp(f))?;
    verify_inserted(&orig, &new, f)?;
    replace_file(path, &md, &new)
}

/// Removes the XMP this tool wrote, restoring the original bytes. `Ok(false)` if there was none.
pub fn remove_our_xmp_from_file(path: &Path) -> Result<bool> {
    let md = fs::metadata(path)?;
    refuse_unwritable(&md)?;
    if xmp_presence(path)? != XmpPresence::Ours {
        return Ok(false);
    }
    let orig = fs::read(path)?;
    let Some(new) = strip_our_xmp(&orig)? else {
        return Ok(false);
    };
    // The result must be a well-formed image without our marker, and strictly smaller.
    if new.len() >= orig.len() || strip_our_xmp(&new)?.is_some() {
        return Err(invalid("verification failed: unexpected result of removing the packet"));
    }
    replace_file(path, &md, &new)?;
    Ok(true)
}
