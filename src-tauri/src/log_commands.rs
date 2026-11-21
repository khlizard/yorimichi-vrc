//! Tauri commands that search the imported log (players, visits, URLs, raw events, avatars,
//! notifications, moderation). JS argument names are camelCase.

use tauri::State;
use yorimichi_core::data::dtos::*;
use yorimichi_core::data::query::QueryService;

use crate::app::{self, AppState};
use crate::commands::{keyset, parse_date, CmdResult};

const ALL_LABEL: &str = "(全て)";

/// "" / "(全て)" / None -> None.
fn filter_opt(s: Option<String>) -> Option<String> {
    s.filter(|v| !v.is_empty() && v != ALL_LABEL)
}

fn with_query<T>(
    state: &AppState,
    f: impl FnOnce(&QueryService) -> yorimichi_core::Result<T>,
) -> CmdResult<T> {
    app::with_query_conn(state, |conn| f(&QueryService::new(conn)))
}

#[tauri::command]
pub async fn recent_players(
    state: State<'_, AppState>,
    from: Option<String>,
    to: Option<String>,
    offset: i64,
    limit: i64,
) -> CmdResult<Vec<PlayerHit>> {
    let (from, to) = (parse_date(from)?, parse_date(to)?);
    with_query(&state, |q| q.get_recent_players(from, to, offset, limit))
}

#[tauri::command]
pub async fn search_players(
    state: State<'_, AppState>,
    term: String,
    offset: i64,
    limit: i64,
) -> CmdResult<Vec<PlayerHit>> {
    with_query(&state, |q| q.search_players(&term, offset, limit))
}

#[tauri::command]
pub async fn encounters(
    state: State<'_, AppState>,
    player_id: i64,
    from: Option<String>,
    to: Option<String>,
) -> CmdResult<Vec<EncounterRow>> {
    let (from, to) = (parse_date(from)?, parse_date(to)?);
    with_query(&state, |q| q.get_encounters(player_id, from, to))
}

#[tauri::command]
pub async fn visits(
    state: State<'_, AppState>,
    world_term: Option<String>,
    from: Option<String>,
    to: Option<String>,
) -> CmdResult<Vec<VisitRow>> {
    let (from, to) = (parse_date(from)?, parse_date(to)?);
    with_query(&state, |q| q.get_visits(world_term.as_deref(), from, to))
}

#[tauri::command]
pub async fn visit_players(state: State<'_, AppState>, visit_id: i64) -> CmdResult<Vec<VisitPlayerRow>> {
    with_query(&state, |q| q.get_visit_players(visit_id))
}

#[tauri::command]
pub async fn visit_urls(state: State<'_, AppState>, visit_id: i64) -> CmdResult<Vec<UrlHit>> {
    with_query(&state, |q| q.get_visit_urls(visit_id))
}

#[tauri::command]
pub async fn search_urls(
    state: State<'_, AppState>,
    term: String,
    kind: Option<String>,
    from: Option<String>,
    to: Option<String>,
) -> CmdResult<Vec<UrlHit>> {
    let (from, to) = (parse_date(from)?, parse_date(to)?);
    let kind = filter_opt(kind);
    with_query(&state, |q| q.search_urls(&term, kind.as_deref(), from, to))
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn search_raw_events(
    state: State<'_, AppState>,
    term: String,
    level: Option<String>,
    from: Option<String>,
    to: Option<String>,
    before_time: Option<String>,
    before_id: Option<i64>,
    limit: i64,
) -> CmdResult<Vec<RawEventHit>> {
    let (from, to) = (parse_date(from)?, parse_date(to)?);
    let level = filter_opt(level);
    let before = keyset(before_time, before_id);
    with_query(&state, |q| q.search_raw_events(&term, level.as_deref(), from, to, before, limit))
}

#[tauri::command]
pub async fn search_avatars(
    state: State<'_, AppState>,
    term: String,
    from: Option<String>,
    to: Option<String>,
) -> CmdResult<Vec<AvatarRow>> {
    let (from, to) = (parse_date(from)?, parse_date(to)?);
    with_query(&state, |q| q.search_avatars(&term, from, to))
}

#[tauri::command]
pub async fn search_notifications(
    state: State<'_, AppState>,
    term: String,
    kind: Option<String>,
    from: Option<String>,
    to: Option<String>,
) -> CmdResult<Vec<NotificationRow>> {
    let (from, to) = (parse_date(from)?, parse_date(to)?);
    let kind = filter_opt(kind);
    with_query(&state, |q| q.search_notifications(&term, kind.as_deref(), from, to))
}

#[tauri::command]
pub async fn notification_kinds(state: State<'_, AppState>) -> CmdResult<Vec<String>> {
    with_query(&state, |q| q.get_notification_kinds())
}

#[tauri::command]
pub async fn search_moderation(
    state: State<'_, AppState>,
    term: String,
    from: Option<String>,
    to: Option<String>,
) -> CmdResult<Vec<ModerationRow>> {
    let (from, to) = (parse_date(from)?, parse_date(to)?);
    with_query(&state, |q| q.search_moderation_events(&term, from, to))
}
