//! Row types of [`super::library::LibraryService`] (memo/tags, worlds, timeline, stats, albums).

use serde::{Deserialize, Serialize};

use super::photo_query::PhotoRow;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Tag {
    pub id: i64,
    pub name: String,
    /// `#rrggbb`
    pub color: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TagCount {
    #[serde(flatten)]
    pub tag: Tag,
    pub player_count: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerProfile {
    pub id: i64,
    pub user_id: String,
    pub display_name: String,
    pub is_local: bool,
    pub memo: String,
    pub tags: Vec<Tag>,
    /// Earliest encounter `join_time`.
    pub first_met: Option<String>,
    /// Latest encounter `join_time`.
    pub last_met: Option<String>,
    pub encounter_count: i64,
    pub total_seconds: i64,
    pub photo_count: i64,
}

/// Memo/tag decoration for list rows. Only players with a memo or at least one tag are returned.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerMark {
    pub player_id: i64,
    pub has_memo: bool,
    pub tags: Vec<Tag>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerBadge {
    pub player_id: i64,
    pub first_met: bool,
    pub days_since_last: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorldRow {
    /// `worlds.id`
    pub id: i64,
    /// `wrld_...`
    pub world_id: String,
    pub name: String,
    pub visit_count: i64,
    pub first_visit: Option<String>,
    pub last_visit: Option<String>,
    pub total_seconds: i64,
    pub memo: String,
    pub favorite: bool,
    pub photo_count: i64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WorldSort {
    #[default]
    LastVisit,
    VisitCount,
    TotalTime,
    Name,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TimelinePlayer {
    pub player_id: i64,
    pub user_id: String,
    pub display_name: String,
    pub join_time: String,
    pub leave_time: String,
    pub first_met: bool,
    pub days_since_last: Option<i64>,
    pub has_memo: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TimelineVisit {
    pub visit_id: i64,
    pub world_name: String,
    pub world_id: Option<String>,
    pub world_ref_id: Option<i64>,
    pub world_favorite: bool,
    pub world_memo: String,
    pub instance_id: String,
    pub location: String,
    pub join_time: String,
    pub leave_time: String,
    pub duration: String,
    pub seconds: i64,
    pub players: Vec<TimelinePlayer>,
    pub photos: Vec<PhotoRow>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TimelineDay {
    pub day: String,
    pub start: String,
    pub end: String,
    pub total_seconds: i64,
    pub visit_count: i64,
    pub people_count: i64,
    pub photo_count: i64,
    pub visits: Vec<TimelineVisit>,
    pub loose_photos: Vec<PhotoRow>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DayActivity {
    pub day: String,
    pub seconds: i64,
    pub visit_count: i64,
    pub people_count: i64,
    pub photo_count: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryYear {
    pub year: i32,
    pub day: String,
    pub visit_count: i64,
    pub people_count: i64,
    pub photo_count: i64,
    pub worlds: Vec<String>,
    pub photos: Vec<PhotoRow>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TopPlayer {
    pub player_id: i64,
    pub user_id: String,
    pub display_name: String,
    pub seconds: i64,
    /// Distinct visits spent together.
    pub count: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TopWorld {
    pub world_ref_id: Option<i64>,
    pub name: String,
    pub seconds: i64,
    pub count: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatsSummary {
    pub total_seconds: i64,
    pub visit_count: i64,
    pub world_count: i64,
    pub people_count: i64,
    pub photo_count: i64,
    pub active_days: i64,
    pub top_players: Vec<TopPlayer>,
    pub top_worlds: Vec<TopWorld>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AlbumRow {
    pub id: i64,
    pub name: String,
    pub description: String,
    /// Explicit cover if it is still in the album and not missing, else the first non-missing photo.
    pub cover_photo_id: Option<i64>,
    /// Photos in the album, including missing ones.
    pub photo_count: i64,
    pub created_at: String,
    pub first_taken: Option<String>,
    pub last_taken: Option<String>,
}
