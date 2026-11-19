//! Self-contained HTML viewer with the encounter data embedded as JSON.
//! Port of `ViewerExporter.cs`.

use std::fmt::Write as _;
use std::path::Path;

use chrono::NaiveDateTime;
use rusqlite::types::ValueRef;
use rusqlite::Connection;

use crate::data::db;
use crate::{Error, Result};

/// Marker in `viewer.html` that is replaced by the JSON data. Must appear exactly once.
pub const PLACEHOLDER: &str = "/*__VRCLOG_DATA__*/null";

pub mod story;

const TEMPLATE: &str = include_str!("viewer.html");

/// Renders the viewer for the current database contents and writes it to `output_path`
/// atomically (`<path>.tmp` then rename over the target). Missing directories are created.
pub fn export(conn: &Connection, output_path: &Path, now: NaiveDateTime) -> Result<()> {
    let template = load_template()?;
    let json = build_json(conn, now)?;
    let html = template.replace(PLACEHOLDER, &json);

    if let Some(dir) = std::path::absolute(output_path)?.parent() {
        std::fs::create_dir_all(dir)?;
    }

    let mut tmp = output_path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = std::path::PathBuf::from(tmp);
    let result = std::fs::write(&tmp, html.as_bytes()).and_then(|_| std::fs::rename(&tmp, output_path));
    if let Err(e) = result {
        let _ = std::fs::remove_file(&tmp); // best effort
        return Err(e.into());
    }
    Ok(())
}

fn load_template() -> Result<&'static str> {
    check_template(TEMPLATE)?;
    Ok(TEMPLATE)
}

fn check_template(template: &str) -> Result<()> {
    if template.matches(PLACEHOLDER).count() != 1 {
        return Err(Error::Template(format!(
            "Embedded resource 'viewer.html' must contain the placeholder '{PLACEHOLDER}' exactly once."
        )));
    }
    Ok(())
}

/// Builds the JSON embedded in the viewer:
/// `{"generatedAt": "...", "players": [[...]], "visits": [[...]], "enc": [[...]], "notif": [[...]]}`
/// where each row is a positional array (column order as in the SQL below).
/// `<`, `>`, `&`, `'`, `+`, `` ` ``, `"` and all non-BMP characters are `\uXXXX`-escaped like
/// `System.Text.Json`'s `JavaScriptEncoder`, so `</script>` can never appear; other
/// non-ASCII text (e.g. Japanese) is kept as is.
pub fn build_json(conn: &Connection, now: NaiveDateTime) -> Result<String> {
    let mut out = String::new();
    out.push_str("{\"generatedAt\":");
    write_json_string(&mut out, &db::ts(now));

    write_array(
        conn,
        &mut out,
        "players",
        r"
        SELECT p.id, p.user_id, p.display_name, p.is_local,
               (SELECT group_concat(pn.display_name, ', ') FROM player_names pn
                 WHERE pn.player_id = p.id AND pn.display_name <> p.display_name)
        FROM players p ORDER BY p.id
        ",
    )?;
    write_array(
        conn,
        &mut out,
        "visits",
        "SELECT id, world_name, instance_id, join_time, leave_time, ended_cleanly FROM visits ORDER BY join_time, id",
    )?;
    write_array(
        conn,
        &mut out,
        "enc",
        "SELECT visit_id, player_id, display_name, join_time, leave_time FROM encounters ORDER BY join_time, id",
    )?;
    write_array(
        conn,
        &mut out,
        "notif",
        "SELECT event_time, kind, sender_name, message, details, accepted_at FROM notifications ORDER BY event_time, id",
    )?;
    out.push('}');
    Ok(out)
}

fn write_array(conn: &Connection, out: &mut String, name: &str, sql: &str) -> Result<()> {
    out.push_str(",\"");
    out.push_str(name);
    out.push_str("\":[");
    let mut stmt = conn.prepare(sql)?;
    let cols = stmt.column_count();
    let mut rows = stmt.query([])?;
    let mut first_row = true;
    while let Some(row) = rows.next()? {
        if !first_row {
            out.push(',');
        }
        first_row = false;
        out.push('[');
        for i in 0..cols {
            if i > 0 {
                out.push(',');
            }
            match row.get_ref(i)? {
                ValueRef::Null => out.push_str("null"),
                ValueRef::Integer(n) => {
                    let _ = write!(out, "{n}");
                }
                ValueRef::Real(d) if d.is_finite() => {
                    let _ = write!(out, "{d}");
                }
                ValueRef::Real(_) => out.push_str("null"),
                ValueRef::Text(t) => write_json_string(out, &String::from_utf8_lossy(t)),
                ValueRef::Blob(b) => write_json_string(out, &String::from_utf8_lossy(b)),
            }
        }
        out.push(']');
    }
    out.push(']');
    Ok(())
}

fn write_json_string(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            '"' | '<' | '>' | '&' | '\'' | '+' | '`' | '\u{0}'..='\u{1f}' | '\u{7f}' | '\u{2028}' | '\u{2029}' => {
                let _ = write!(out, "\\u{:04X}", c as u32);
            }
            c if (c as u32) > 0xFFFF => {
                let mut buf = [0u16; 2];
                for u in c.encode_utf16(&mut buf) {
                    let _ = write!(out, "\\u{:04X}", u);
                }
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_template_has_one_placeholder() {
        assert!(load_template().is_ok());
    }

    #[test]
    fn template_check_rejects_zero_or_two_placeholders() {
        assert!(check_template("<html></html>").is_err());
        assert!(check_template(&format!("{PLACEHOLDER}{PLACEHOLDER}")).is_err());
    }

    #[test]
    fn json_strings_escape_html_sensitive_chars() {
        let mut s = String::new();
        write_json_string(&mut s, "</script>&'+\"\\テ😀");
        assert_eq!(s, "\"\\u003C/script\\u003E\\u0026\\u0027\\u002B\\u0022\\\\テ\\uD83D\\uDE00\"");
    }
}
