//! Shareable "story" pages: one album or one day as a self-contained HTML file with the photos
//! embedded as base64 JPEG. No file paths, user ids or world ids are written; names only.
//!
//! The page is streamed (template head, per-item HTML, template tail) so hundreds of photos
//! never sit in one `String`. Photo files are only read.

use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use base64::Engine as _;
use chrono::{NaiveDate, NaiveDateTime};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::data::library_dtos::{TimelineDay, TimelineVisit};
use crate::data::photo_query::{PhotoRow, PhotoService};
use crate::data::LibraryService;
use crate::photos::thumbs::jpeg_bytes;
use crate::{Error, Result};

const TEMPLATE: &str = include_str!("story.html");
const M_TITLE: &str = "<!--__STORY_TITLE__-->";
const M_BODY: &str = "<!--__STORY_BODY__-->";
const M_GENERATED: &str = "<!--__STORY_GENERATED__-->";
const JPEG_QUALITY: u8 = 85;
const DEFAULT_MAX_EDGE: u32 = 1600;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct StoryOptions {
    /// Write the names of the people met.
    pub include_players: bool,
    /// Long side of the embedded JPEGs in pixels (0 = default).
    pub max_edge: u32,
}

impl Default for StoryOptions {
    fn default() -> Self {
        Self { include_players: true, max_edge: DEFAULT_MAX_EDGE }
    }
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportSummary {
    pub photos_embedded: usize,
    pub photos_skipped: usize,
}

/// Escapes text for HTML element content and attribute values.
fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}

/// `HH:MM` of a stored timestamp.
fn hm(ts: &str) -> &str {
    ts.get(11..16).unwrap_or("")
}

fn ymd_hm(ts: &str) -> &str {
    ts.get(..16).unwrap_or(ts)
}

fn fmt_secs(secs: i64) -> String {
    format!("{}h {:02}m", secs / 3600, (secs % 3600) / 60)
}

fn names(list: impl Iterator<Item = impl AsRef<str>>) -> String {
    list.map(|n| esc(n.as_ref())).collect::<Vec<_>>().join("、")
}

/// Splits the template around the body marker, filling title and generation time.
fn split_template(title: &str, generated: &str) -> Result<(String, String)> {
    for m in [M_TITLE, M_BODY, M_GENERATED] {
        if TEMPLATE.matches(m).count() != 1 {
            return Err(Error::Template(format!("story.html must contain '{m}' exactly once")));
        }
    }
    let (head, tail) = TEMPLATE.split_once(M_BODY).expect("marker was checked");
    Ok((head.replace(M_TITLE, &esc(title)), tail.replace(M_GENERATED, &esc(generated))))
}

/// Writes `head + body + tail` to `<out>.tmp` and renames it over `out`.
fn write_page(
    out: &Path,
    title: &str,
    now: NaiveDateTime,
    body: impl FnOnce(&mut BufWriter<File>) -> Result<ExportSummary>,
) -> Result<ExportSummary> {
    let (head, tail) = split_template(title, &now.format("%Y-%m-%d %H:%M").to_string())?;
    if let Some(dir) = std::path::absolute(out)?.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut tmp = out.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);

    let run = || -> Result<ExportSummary> {
        let mut w = BufWriter::new(File::create(&tmp)?);
        w.write_all(head.as_bytes())?;
        let summary = body(&mut w)?;
        w.write_all(tail.as_bytes())?;
        w.flush()?;
        drop(w);
        std::fs::rename(&tmp, out)?;
        Ok(summary)
    };
    let result = run();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp); // best effort
    }
    result
}

/// Writes one `<figure>` with the embedded photo. Returns `false` (nothing written) when the photo
/// is missing, online-only or cannot be decoded.
fn write_figure(w: &mut impl Write, photo: &PhotoRow, alt: &str, caption_html: &str, max_edge: u32) -> Result<bool> {
    if photo.missing || photo.online_only {
        return Ok(false);
    }
    // The flag dates from the last scan; re-check so a file that became a cloud placeholder
    // since then is not downloaded by reading it.
    if !std::fs::metadata(&photo.path).is_ok_and(|md| md.is_file() && !crate::photos::is_online_only(&md)) {
        return Ok(false);
    }
    let Ok(bytes) = jpeg_bytes(Path::new(&photo.path), max_edge, JPEG_QUALITY) else {
        return Ok(false);
    };
    let b64 = base64::engine::general_purpose::STANDARD.encode(bytes);
    write!(w, "<figure><img src=\"data:image/jpeg;base64,{b64}\" alt=\"{}\" loading=\"lazy\">", esc(alt))?;
    if !caption_html.is_empty() {
        write!(w, "<figcaption>{caption_html}</figcaption>")?;
    }
    writeln!(w, "</figure>")?;
    Ok(true)
}

fn edge(opts: &StoryOptions) -> u32 {
    if opts.max_edge == 0 {
        DEFAULT_MAX_EDGE
    } else {
        opts.max_edge
    }
}

/// Exports an album: title, description, date range, then the photos in album order, each with a
/// one-line caption (time, world and, with `include_players`, the people present).
pub fn export_album(
    conn: &Connection,
    album_id: i64,
    out: &Path,
    opts: &StoryOptions,
    now: NaiveDateTime,
) -> Result<ExportSummary> {
    let lib = LibraryService::new(conn);
    let album = lib.album(album_id)?.ok_or_else(|| Error::Invalid("album not found".into()))?;
    let photos = lib.album_photos(album_id)?;
    let svc = PhotoService::new(conn);
    let max_edge = edge(opts);

    // Resolved up front so a database error cannot leave a half-written page behind.
    let mut who: Vec<String> = Vec::with_capacity(photos.len());
    for p in &photos {
        let text = if opts.include_players {
            match svc.photo_detail(p.id)? {
                Some(d) => names(d.players.iter().filter(|pl| !pl.is_local).map(|pl| pl.display_name.as_str())),
                None => String::new(),
            }
        } else {
            String::new()
        };
        who.push(text);
    }

    write_page(out, &album.name, now, |w| {
        let mut summary = ExportSummary::default();
        writeln!(w, "<header><h1>{}</h1>", esc(&album.name))?;
        if let (Some(a), Some(b)) = (&album.first_taken, &album.last_taken) {
            let (a, b) = (a.get(..10).unwrap_or(a), b.get(..10).unwrap_or(b));
            let range = if a == b { a.to_string() } else { format!("{a} – {b}") };
            writeln!(w, "<p class=\"sub\">{}</p>", esc(&range))?;
        }
        if !album.description.is_empty() {
            writeln!(w, "<p class=\"desc\">{}</p>", esc(&album.description))?;
        }
        writeln!(w, "</header>\n<div class=\"album\">")?;
        for (p, people) in photos.iter().zip(&who) {
            let world = p.world_name.as_deref().unwrap_or("");
            let mut caption = esc(ymd_hm(&p.taken_at));
            if !world.is_empty() {
                caption.push_str(&format!(" · {}", esc(world)));
            }
            if !people.is_empty() {
                caption.push_str(&format!(" · {people}"));
            }
            let alt = if world.is_empty() { ymd_hm(&p.taken_at).to_string() } else { format!("{world} {}", ymd_hm(&p.taken_at)) };
            if write_figure(w, p, &alt, &caption, max_edge)? {
                summary.photos_embedded += 1;
            } else {
                summary.photos_skipped += 1;
            }
        }
        writeln!(w, "</div>")?;
        Ok(summary)
    })
}

/// A rail entry: a visit section or a loose photo, merged by time.
enum Stop<'a> {
    Visit(&'a TimelineVisit),
    Photo(&'a PhotoRow),
}

fn day_stops(day: &TimelineDay) -> Vec<(&str, Stop<'_>)> {
    let mut stops: Vec<(&str, Stop<'_>)> = day.visits.iter().map(|v| (v.join_time.as_str(), Stop::Visit(v))).collect();
    stops.extend(day.loose_photos.iter().map(|p| (p.taken_at.as_str(), Stop::Photo(p))));
    stops.sort_by(|a, b| a.0.cmp(b.0)); // stable: visits stay before photos of the same second
    stops
}

/// Exports one logical day as a vertical time rail: per visit a section, loose photos in between.
pub fn export_day(
    conn: &Connection,
    day: NaiveDate,
    day_start_hour: u32,
    out: &Path,
    opts: &StoryOptions,
    now: NaiveDateTime,
) -> Result<ExportSummary> {
    let tl = LibraryService::new(conn).timeline_day(day, day_start_hour, 0, false)?;
    let max_edge = edge(opts);
    let title = format!("{} の記録", tl.day);

    write_page(out, &title, now, |w| {
        let mut summary = ExportSummary::default();
        let mut count = |embedded: bool| {
            if embedded {
                summary.photos_embedded += 1;
            } else {
                summary.photos_skipped += 1;
            }
        };

        write!(w, "<header><h1>{}</h1>", esc(&title))?;
        if tl.visit_count > 0 {
            let mut sub = format!("訪問 {} 件", tl.visit_count);
            if opts.include_players {
                sub.push_str(&format!(" · 会った人 {} 人", tl.people_count));
            }
            sub.push_str(&format!(" · 合計 {}", fmt_secs(tl.total_seconds)));
            write!(w, "<p class=\"sub\">{}</p>", esc(&sub))?;
        }
        writeln!(w, "</header>")?;

        let stops = day_stops(&tl);
        if stops.is_empty() {
            writeln!(w, "<p class=\"empty\">この日の記録はありません。</p>")?;
        } else {
            writeln!(w, "<div class=\"rail\">")?;
        }
        for (_, stop) in &stops {
            match stop {
                Stop::Visit(v) => {
                    writeln!(w, "<section class=\"stop\"><div class=\"when\">{}</div>", esc(hm(&v.join_time)))?;
                    writeln!(w, "<h2>{}</h2>", esc(&v.world_name))?;
                    let mut meta = format!("{} – {}", hm(&v.join_time), hm(&v.leave_time));
                    if !v.duration.is_empty() {
                        meta.push_str(&format!(" · {}", v.duration));
                    }
                    writeln!(w, "<p class=\"meta\">{}</p>", esc(&meta))?;
                    if opts.include_players && !v.players.is_empty() {
                        writeln!(
                            w,
                            "<p class=\"who\">会った人: {}</p>",
                            names(v.players.iter().map(|p| p.display_name.as_str()))
                        )?;
                    }
                    for p in &v.photos {
                        let alt = format!("{} {}", v.world_name, hm(&p.taken_at));
                        count(write_figure(w, p, &alt, &esc(hm(&p.taken_at)), max_edge)?);
                    }
                    writeln!(w, "</section>")?;
                }
                Stop::Photo(p) => {
                    // Written to a buffer first so a skipped photo leaves no empty rail stop.
                    let mut buf = Vec::new();
                    let alt = hm(&p.taken_at).to_string();
                    let ok = write_figure(&mut buf, p, &alt, "", max_edge)?;
                    count(ok);
                    if ok {
                        writeln!(w, "<section class=\"stop\"><div class=\"when\">{}</div>", esc(hm(&p.taken_at)))?;
                        w.write_all(&buf)?;
                        writeln!(w, "</section>")?;
                    }
                }
            }
        }
        if !stops.is_empty() {
            writeln!(w, "</div>")?;
        }
        Ok(summary)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn template_markers_are_unique() {
        assert!(split_template("t", "g").is_ok());
    }

    #[test]
    fn escapes_html() {
        assert_eq!(esc("<a href=\"x\">&'"), "&lt;a href=&quot;x&quot;&gt;&amp;&#39;");
    }
}
