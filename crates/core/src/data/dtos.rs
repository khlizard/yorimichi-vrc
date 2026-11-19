//! Row types returned by [`super::query::QueryService`]. Port of `Dtos.cs`.
//!
//! All structs serialize to camelCase JSON. Members that were computed getters in C#
//! (`LastSeenDate`, `Duration`, `Response`) are real, pre-computed fields here so the UI
//! receives them in the JSON.

use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};

const TS_FORMAT: &str = "%Y-%m-%d %H:%M:%S";

/// `Xh MMm` when at least an hour, otherwise `Xm SSs`; empty if unparsable or leave < join.
pub fn format_duration(join: &str, leave: &str) -> String {
    let (Ok(j), Ok(l)) = (
        NaiveDateTime::parse_from_str(join, TS_FORMAT),
        NaiveDateTime::parse_from_str(leave, TS_FORMAT),
    ) else {
        return String::new();
    };
    if l < j {
        return String::new();
    }
    let secs = (l - j).num_seconds();
    let hours = secs / 3600;
    let minutes = (secs % 3600) / 60;
    let seconds = secs % 60;
    if hours >= 1 {
        format!("{hours}h {minutes:02}m")
    } else {
        format!("{minutes}m {seconds:02}s")
    }
}

/// First 10 chars (`yyyy-MM-dd`) of a timestamp, or empty.
pub fn last_seen_date(last_seen: Option<&str>) -> String {
    match last_seen {
        Some(ls) if ls.chars().count() >= 10 => ls.chars().take(10).collect(),
        _ => String::new(),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerHit {
    pub id: i64,
    pub user_id: String,
    pub display_name: String,
    pub is_local: bool,
    pub other_names: Option<String>,
    pub encounter_count: i32,
    pub last_seen: Option<String>,
    /// `yyyy-MM-dd` part of `last_seen`, or empty.
    pub last_seen_date: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EncounterRow {
    pub visit_id: i64,
    pub display_name: String,
    pub world_name: String,
    pub instance_id: String,
    pub join_time: String,
    pub leave_time: String,
    pub ended_cleanly: bool,
    pub avatar_name: Option<String>,
    pub avatar_size_mb: Option<f64>,
    pub duration: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VisitRow {
    pub id: i64,
    pub world_name: String,
    pub instance_id: String,
    pub location: String,
    pub join_time: String,
    pub leave_time: String,
    pub ended_cleanly: bool,
    pub player_count: i32,
    pub duration: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VisitPlayerRow {
    pub player_id: i64,
    pub display_name: String,
    pub is_local: bool,
    pub join_time: String,
    pub leave_time: String,
    pub ended_cleanly: bool,
    pub avatar_name: Option<String>,
    pub avatar_size_mb: Option<f64>,
    pub duration: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AvatarRow {
    pub visit_id: Option<i64>,
    pub event_time: String,
    pub display_name: String,
    pub avatar_name: String,
    pub size_mb: Option<f64>,
    pub load_seconds: Option<i32>,
    pub world_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UrlHit {
    pub visit_id: Option<i64>,
    pub event_time: String,
    pub kind: String,
    pub url: String,
    pub world_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawEventHit {
    pub id: i64,
    pub visit_id: Option<i64>,
    pub event_time: String,
    pub level: String,
    pub message: String,
    pub world_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotificationRow {
    pub id: i64,
    pub notification_id: String,
    pub event_time: String,
    pub kind: String,
    pub sender_user_id: String,
    pub sender_name: String,
    pub message: String,
    pub details: String,
    pub accepted_at: Option<String>,
    /// `"応答済"` when `accepted_at` is set, otherwise empty.
    pub response: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModerationRow {
    pub visit_id: Option<i64>,
    pub event_time: String,
    pub kind: String,
    pub target_name: String,
    pub world_name: String,
}
