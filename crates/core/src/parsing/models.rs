//! Parsed (in-memory) log models.

use chrono::NaiveDateTime;

use super::noise;

#[derive(Debug, Clone, PartialEq)]
pub struct ParsedEncounter {
    pub user_id: String,
    pub display_name: String,
    pub join_time: NaiveDateTime,
    pub leave_time: NaiveDateTime,
    pub ended_cleanly: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParsedUrlEvent {
    /// "video" | "string" | "image"
    pub kind: String,
    pub url: String,
    pub time: NaiveDateTime,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParsedRawEvent {
    pub time: NaiveDateTime,
    /// "Debug" | "Warning" | ...
    pub level: String,
    pub message: String,
    /// Index into `ParseResult::visits`, `None` if outside any visit.
    pub visit_index: Option<usize>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParsedVisit {
    pub world_id: Option<String>,
    pub world_name: String,
    pub instance_id: String,
    pub location: String,
    pub join_time: NaiveDateTime,
    pub leave_time: NaiveDateTime,
    pub ended_cleanly: bool,
    pub encounters: Vec<ParsedEncounter>,
    pub url_events: Vec<ParsedUrlEvent>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParsedNotification {
    /// `not_...` / `frq_...`
    pub notification_id: String,
    /// invite | requestInvite | friendRequest | boop | message | ...
    pub kind: String,
    pub sender_user_id: String,
    pub sender_name: String,
    /// First line only.
    pub message: String,
    /// Raw `k=v, k=v` text from the details block.
    pub details: String,
    /// As written in the log, e.g. `06/23/2026 12:40:24`.
    pub created_at_utc: String,
    /// Log line timestamp (local).
    pub time: NaiveDateTime,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParsedModerationEvent {
    /// mute | unmute | block | unblock | hide | unhide | votekick | ...
    pub kind: String,
    pub target_name: String,
    pub time: NaiveDateTime,
    pub visit_index: Option<usize>,
}

/// A player (identified by display name only) wearing an avatar; size is known only when it was downloaded.
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedAvatarEvent {
    pub player_name: String,
    pub avatar_name: String,
    pub time: NaiveDateTime,
    pub visit_index: Option<usize>,
    pub size_mb: Option<f64>,
    pub load_seconds: Option<i32>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParseResult {
    pub local_user_id: Option<String>,
    pub local_user_name: Option<String>,
    pub visits: Vec<ParsedVisit>,
    pub orphan_url_events: Vec<ParsedUrlEvent>,
    pub raw_events: Vec<ParsedRawEvent>,
    pub notifications: Vec<ParsedNotification>,
    /// (notification id, time)
    pub answered_notifications: Vec<(String, NaiveDateTime)>,
    pub moderation_events: Vec<ParsedModerationEvent>,
    pub avatar_events: Vec<ParsedAvatarEvent>,
    pub last_timestamp: Option<NaiveDateTime>,
}

/// Options controlling what the parser keeps.
#[derive(Debug, Clone)]
pub struct ParseOptions {
    pub raw_noise_patterns: Vec<String>,
}

impl Default for ParseOptions {
    fn default() -> Self {
        Self { raw_noise_patterns: noise::default_patterns() }
    }
}

impl ParseOptions {
    pub fn new<S: Into<String>>(patterns: impl IntoIterator<Item = S>) -> Self {
        Self { raw_noise_patterns: patterns.into_iter().map(Into::into).collect() }
    }
}
