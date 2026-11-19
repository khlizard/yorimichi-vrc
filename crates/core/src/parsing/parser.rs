//! Line-based state machine over a VRChat log. Port of `LogParser.cs`.

use std::collections::HashMap;
use std::path::Path;

use chrono::{Duration, NaiveDateTime};

use super::models::*;
use super::noise;
use super::regex as re;

const URL_DEDUPE_WINDOW_SECS: i64 = 10;

/// Splits text the way .NET `TextReader.ReadLine` does: on `\n`, `\r\n` and a lone `\r`.
/// A trailing terminator does not produce an extra empty line. A leading BOM is skipped.
fn split_lines(text: &str) -> Vec<&str> {
    let text = text.strip_prefix('\u{FEFF}').unwrap_or(text);
    let bytes = text.as_bytes();
    let mut lines = Vec::new();
    let mut start = 0;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\n' => {
                lines.push(&text[start..i]);
                i += 1;
                start = i;
            }
            b'\r' => {
                lines.push(&text[start..i]);
                i += 1;
                if i < bytes.len() && bytes[i] == b'\n' {
                    i += 1;
                }
                start = i;
            }
            _ => i += 1,
        }
    }
    if start < bytes.len() {
        lines.push(&text[start..]);
    }
    lines
}

/// Reads a log file (shared read, so a file VRChat is still writing works) and parses it.
/// Returns the parse result together with the byte length of the file when it was opened.
pub fn parse_file(path: &Path, options: &ParseOptions) -> std::io::Result<(ParseResult, u64)> {
    use std::io::Read;
    // Rust's std opens files with FILE_SHARE_READ | WRITE | DELETE on Windows.
    let mut file = std::fs::File::open(path)?;
    let size = file.metadata()?.len();
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    Ok((parse_bytes(&bytes, options), size))
}

/// Parses raw bytes (decoded as UTF-8, invalid sequences replaced).
pub fn parse_bytes(bytes: &[u8], options: &ParseOptions) -> ParseResult {
    parse(&String::from_utf8_lossy(bytes), options)
}

struct VisitBuilder {
    world_id: String,
    world_name: String,
    instance_id: String,
    location: String,
    join_time: NaiveDateTime,
    /// Open encounters in insertion order: (user id, display name, join time).
    open: Vec<(String, String, NaiveDateTime)>,
    encounters: Vec<ParsedEncounter>,
    urls: Vec<ParsedUrlEvent>,
    url_last_seen: HashMap<(String, String), NaiveDateTime>,
    leave_time: NaiveDateTime,
    ended_cleanly: bool,
    closed: bool,
}

impl VisitBuilder {
    fn open_encounter(&mut self, user_id: &str, name: &str, ts: NaiveDateTime) {
        if !self.open.iter().any(|(u, _, _)| u == user_id) {
            self.open.push((user_id.to_string(), name.to_string(), ts));
        }
    }

    fn close_encounter(&mut self, user_id: &str, ts: NaiveDateTime, ended_cleanly: bool) {
        if let Some(pos) = self.open.iter().position(|(u, _, _)| u == user_id) {
            let (uid, name, join) = self.open.remove(pos);
            self.encounters.push(ParsedEncounter {
                user_id: uid,
                display_name: name,
                join_time: join,
                leave_time: ts,
                ended_cleanly,
            });
        }
    }

    fn add_url(&mut self, kind: &str, url: &str, ts: NaiveDateTime, window: Duration) {
        let key = (kind.to_string(), url.to_string());
        if let Some(seen) = self.url_last_seen.get(&key) {
            if ts - *seen <= window {
                self.url_last_seen.insert(key, ts);
                return;
            }
        }
        self.url_last_seen.insert(key, ts);
        self.urls.push(ParsedUrlEvent { kind: kind.to_string(), url: url.to_string(), time: ts });
    }

    fn close(&mut self, ts: NaiveDateTime, ended_cleanly: bool) {
        if self.closed {
            return;
        }
        let ids: Vec<String> = self.open.iter().map(|(u, _, _)| u.clone()).collect();
        for id in ids {
            self.close_encounter(&id, ts, ended_cleanly);
        }
        self.leave_time = ts;
        self.ended_cleanly = ended_cleanly;
        self.closed = true;
    }

    fn build(mut self) -> ParsedVisit {
        self.encounters.sort_by_key(|e| e.join_time); // stable, like LINQ OrderBy
        ParsedVisit {
            world_id: Some(self.world_id),
            world_name: self.world_name,
            instance_id: self.instance_id,
            location: self.location,
            join_time: self.join_time,
            leave_time: if self.closed { self.leave_time } else { self.join_time },
            ended_cleanly: self.ended_cleanly,
            encounters: self.encounters,
            url_events: self.urls,
        }
    }
}

/// Parses the full text of a log. Lines that do not start with a timestamp prefix
/// (stack traces etc.) are ignored.
pub fn parse(text: &str, options: &ParseOptions) -> ParseResult {
    let window = Duration::seconds(URL_DEDUPE_WINDOW_SECS);
    let mut local_user_id: Option<String> = None;
    let mut local_user_name: Option<String> = None;
    let mut visits: Vec<VisitBuilder> = Vec::new();
    // The visit currently open is always the last one in `visits`.
    let mut in_visit = false;
    let mut orphan_urls: Vec<ParsedUrlEvent> = Vec::new();
    let mut orphan_url_last_seen: HashMap<(String, String), NaiveDateTime> = HashMap::new();
    let mut raw_events: Vec<ParsedRawEvent> = Vec::new();
    let mut notifications: Vec<ParsedNotification> = Vec::new();
    let mut answered: Vec<(String, NaiveDateTime)> = Vec::new();
    let mut moderation: Vec<ParsedModerationEvent> = Vec::new();
    let mut avatars: Vec<ParsedAvatarEvent> = Vec::new();
    let mut pending_download: Option<(f64, i32)> = None;
    let mut pending_world_name: Option<String> = None;
    let mut last_timestamp: Option<NaiveDateTime> = None;

    for line in split_lines(text) {
        let Some(prefix) = re::LINE_PREFIX.captures(line) else {
            continue; // continuation line (stack trace etc.)
        };
        // An impossible date (e.g. month 13) is skipped instead of aborting the whole file.
        let Ok(ts) = NaiveDateTime::parse_from_str(&prefix[1], "%Y.%m.%d %H:%M:%S") else {
            continue;
        };
        let level = &prefix[2];
        let message = &prefix[3];
        last_timestamp = Some(ts);

        // Handle visit transitions before recording the raw event so the
        // "Joining wrld_..." line is attributed to the visit it starts.
        let joining = re::JOINING_WORLD.captures(message);
        if let Some(j) = &joining {
            if in_visit {
                visits.last_mut().unwrap().close(ts, false); // missed OnLeftRoom
            }
            visits.push(VisitBuilder {
                world_id: j[1].to_string(),
                world_name: pending_world_name.take().unwrap_or_default(),
                instance_id: j[2].to_string(),
                location: format!("{}:{}", &j[1], &j[2]),
                join_time: ts,
                open: Vec::new(),
                encounters: Vec::new(),
                urls: Vec::new(),
                url_last_seen: HashMap::new(),
                leave_time: ts,
                ended_cleanly: false,
                closed: false,
            });
            in_visit = true;
            pending_world_name = None;
        }

        let visit_index = if in_visit { Some(visits.len() - 1) } else { None };
        if level != "Error" && !noise::is_noise(message, &options.raw_noise_patterns) {
            raw_events.push(ParsedRawEvent {
                time: ts,
                level: level.to_string(),
                message: message.to_string(),
                visit_index,
            });
        }

        if joining.is_some() {
            continue;
        }

        if let Some(m) = re::NOTIFICATION.captures(message) {
            let kind = m[4].trim().to_string();
            if kind != "group" {
                // group bulletins are bulk broadcasts, not personal communication
                let mut text = m[8].to_string();
                if text.ends_with("\">") {
                    text.truncate(text.len() - 2);
                }
                notifications.push(ParsedNotification {
                    notification_id: m[5].trim().to_string(),
                    kind,
                    sender_user_id: m[2].to_string(),
                    sender_name: m[1].to_string(),
                    message: text,
                    details: m[7].to_string(),
                    created_at_utc: m[6].trim().to_string(),
                    time: ts,
                });
            }
            continue;
        }

        if let Some(m) = re::NOTIFICATION_ANSWERED.captures(message) {
            answered.push((m[1].trim().to_string(), ts));
            continue;
        }

        if let Some(m) = re::MODERATION.captures(message) {
            let word = m[3].to_lowercase();
            let base_kind = match word.as_str() {
                "muted" => "mute".to_string(),
                "blocked" => "block".to_string(),
                "hidden" => "hide".to_string(),
                _ => word,
            };
            let kind = if &m[2] == "now" { base_kind } else { format!("un{base_kind}") };
            moderation.push(ParsedModerationEvent {
                kind,
                target_name: m[1].to_string(),
                time: ts,
                visit_index,
            });
            continue;
        }

        if let Some(m) = re::VOTE_KICK.captures(message) {
            moderation.push(ParsedModerationEvent {
                kind: "votekick".to_string(),
                target_name: m[1].to_string(),
                time: ts,
                visit_index,
            });
            continue;
        }

        if let Some(m) = re::AVATAR_SWITCH.captures(message) {
            let player = &m[1];
            let avatar = &m[2];
            // Joining a world logs one "Switching" per player present; re-logging the same
            // avatar for the same player within a visit adds nothing.
            let last = avatars.iter().rev().find(|a| a.player_name == player);
            if last.is_none_or(|l| l.visit_index != visit_index || l.avatar_name != avatar) {
                avatars.push(ParsedAvatarEvent {
                    player_name: player.to_string(),
                    avatar_name: avatar.to_string(),
                    time: ts,
                    visit_index,
                    size_mb: None,
                    load_seconds: None,
                });
            }
            continue;
        }

        if let Some(m) = re::AVATAR_DOWNLOAD.captures(message) {
            // The matching "Unpacking Avatar (X by NAME)" line follows right after this one.
            if let (Ok(size), Ok(secs)) = (m[1].parse::<f64>(), m[3].parse::<i32>()) {
                pending_download = Some((size, secs));
            }
            continue;
        }

        if let Some(m) = re::AVATAR_UNPACK.captures(message) {
            if let Some((size_mb, load_seconds)) = pending_download {
                let avatar = &m[1];
                let player = &m[2];
                let idx = avatars
                    .iter()
                    .rposition(|a| a.player_name == player && a.avatar_name == avatar)
                    .or_else(|| avatars.iter().rposition(|a| a.player_name == player));
                let idx = match idx {
                    Some(i) => i,
                    None => {
                        avatars.push(ParsedAvatarEvent {
                            player_name: player.to_string(),
                            avatar_name: avatar.to_string(),
                            time: ts,
                            visit_index,
                            size_mb: None,
                            load_seconds: None,
                        });
                        avatars.len() - 1
                    }
                };
                avatars[idx].size_mb = Some(size_mb);
                avatars[idx].load_seconds = Some(load_seconds);
                pending_download = None;
            }
            continue;
        }

        if let Some(m) = re::PLAYER_JOINED.captures(message) {
            if in_visit {
                visits.last_mut().unwrap().open_encounter(&m[2], &m[1], ts);
            }
            continue;
        }

        if let Some(m) = re::PLAYER_LEFT.captures(message) {
            if in_visit {
                visits.last_mut().unwrap().close_encounter(&m[2], ts, true);
            }
            continue;
        }

        if let Some(m) = re::ENTERING_ROOM.captures(message) {
            pending_world_name = Some(m[1].to_string());
            continue;
        }

        if re::LEFT_ROOM.is_match(message) {
            if in_visit {
                visits.last_mut().unwrap().close(ts, true);
            }
            in_visit = false;
            continue;
        }

        if let Some(m) = re::AUTHENTICATED.captures(message) {
            local_user_name = Some(m[1].to_string());
            local_user_id = Some(m[2].to_string());
            continue;
        }

        let found = if let Some(m) = re::VIDEO_URL.captures(message) {
            Some(("video", m[1].to_string()))
        } else if let Some(m) = re::STRING_URL.captures(message) {
            Some(("string", m[1].to_string()))
        } else {
            re::IMAGE_URL.captures(message).map(|m| ("image", m[1].to_string()))
        };
        if let Some((k, url)) = found {
            if in_visit {
                visits.last_mut().unwrap().add_url(k, &url, ts, window);
            } else {
                let key = (k.to_string(), url.clone());
                let stale = match orphan_url_last_seen.get(&key) {
                    Some(seen) => ts - *seen > window,
                    None => true,
                };
                if stale {
                    orphan_url_last_seen.insert(key, ts);
                    orphan_urls.push(ParsedUrlEvent { kind: k.to_string(), url, time: ts });
                }
            }
        }
    }

    // Log ended abruptly (crash / VRChat still running): close everything at the last seen time.
    if in_visit {
        if let Some(eof) = last_timestamp {
            visits.last_mut().unwrap().close(eof, false);
        }
    }

    ParseResult {
        local_user_id,
        local_user_name,
        visits: visits.into_iter().map(VisitBuilder::build).collect(),
        orphan_url_events: orphan_urls,
        raw_events,
        notifications,
        answered_notifications: answered,
        moderation_events: moderation,
        avatar_events: avatars,
        last_timestamp,
    }
}
