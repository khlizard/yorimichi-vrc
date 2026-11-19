//! Tauri commands for the library features (tags/memos, worlds, timeline, stats, albums, story export, links).

use std::path::PathBuf;

use chrono::{Local, NaiveDate, NaiveDateTime};
use tauri::{AppHandle, Manager, State};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;
use vrclogger_core::data::dtos::PlayerHit;
use vrclogger_core::data::library::LibraryService;
use vrclogger_core::data::library_dtos::*;
use vrclogger_core::data::photo_query::{PhotoRow, PhotoService};
use vrclogger_core::export::story::{self, ExportSummary, StoryOptions};

use crate::app::{self, AppState};
use crate::commands::{parse_date, CmdResult};
use crate::settings::settings_dir;

const NO_DB: &str = "データベースを開けません。設定の DB パスを確認してください。";

fn with_library<T>(
    state: &AppState,
    f: impl FnOnce(&LibraryService) -> vrclogger_core::Result<T>,
) -> CmdResult<T> {
    let guard = state.query.lock().unwrap();
    let conn = guard.as_ref().ok_or_else(|| NO_DB.to_string())?;
    f(&LibraryService::new(conn)).map_err(|e| e.to_string())
}

fn now() -> NaiveDateTime {
    Local::now().naive_local()
}

fn require_date(s: &str) -> CmdResult<NaiveDate> {
    parse_date(Some(s.to_string()))?.ok_or_else(|| "日付を指定してください".to_string())
}

fn day_start_hour(state: &AppState) -> u32 {
    state.settings.lock().unwrap().day_start_hour
}

// ---- Tags / players ----

#[tauri::command]
pub async fn list_tags(state: State<'_, AppState>) -> CmdResult<Vec<TagCount>> {
    with_library(&state, |l| l.list_tags())
}

#[tauri::command]
pub async fn create_tag(state: State<'_, AppState>, name: String, color: String) -> CmdResult<Tag> {
    with_library(&state, |l| l.create_tag(&name, &color))
}

#[tauri::command]
pub async fn update_tag(state: State<'_, AppState>, id: i64, name: String, color: String) -> CmdResult<()> {
    with_library(&state, |l| l.update_tag(id, &name, &color))
}

#[tauri::command]
pub async fn delete_tag(state: State<'_, AppState>, id: i64) -> CmdResult<()> {
    with_library(&state, |l| l.delete_tag(id))
}

#[tauri::command]
pub async fn set_player_tags(state: State<'_, AppState>, player_id: i64, tag_ids: Vec<i64>) -> CmdResult<()> {
    with_library(&state, |l| l.set_player_tags(player_id, &tag_ids))
}

#[tauri::command]
pub async fn set_player_memo(state: State<'_, AppState>, player_id: i64, memo: String) -> CmdResult<()> {
    with_library(&state, |l| l.set_player_memo(player_id, &memo, now()))
}

#[tauri::command]
pub async fn player_profile(state: State<'_, AppState>, player_id: i64) -> CmdResult<Option<PlayerProfile>> {
    with_library(&state, |l| l.player_profile(player_id))
}

#[tauri::command]
pub async fn players_by_tag(state: State<'_, AppState>, tag_id: i64) -> CmdResult<Vec<PlayerHit>> {
    with_library(&state, |l| l.players_by_tag(tag_id))
}

#[tauri::command]
pub async fn player_marks(state: State<'_, AppState>, player_ids: Vec<i64>) -> CmdResult<Vec<PlayerMark>> {
    with_library(&state, |l| l.player_marks(&player_ids))
}

#[tauri::command]
pub async fn visit_player_badges(state: State<'_, AppState>, visit_id: i64) -> CmdResult<Vec<PlayerBadge>> {
    let (reunion_days, show_first_met) = {
        let s = state.settings.lock().unwrap();
        (s.reunion_days, s.show_first_met)
    };
    let badges = with_library(&state, |l| l.visit_player_badges(visit_id, reunion_days))?;
    Ok(filter_badges(badges, show_first_met))
}

fn filter_badges(badges: Vec<PlayerBadge>, show_first_met: bool) -> Vec<PlayerBadge> {
    if show_first_met {
        return badges;
    }
    badges
        .into_iter()
        .map(|mut b| {
            b.first_met = false;
            b
        })
        .filter(|b| b.days_since_last.is_some())
        .collect()
}

// ---- Worlds ----

#[tauri::command]
pub async fn worlds(
    state: State<'_, AppState>,
    term: Option<String>,
    favorite_only: bool,
    sort: WorldSort,
) -> CmdResult<Vec<WorldRow>> {
    let term = term.map(|t| t.trim().to_string()).filter(|t| !t.is_empty());
    with_library(&state, |l| l.worlds(term.as_deref(), favorite_only, sort))
}

#[tauri::command]
pub async fn world(state: State<'_, AppState>, id: i64) -> CmdResult<Option<WorldRow>> {
    with_library(&state, |l| l.world(id))
}

#[tauri::command]
pub async fn set_world_note(
    state: State<'_, AppState>,
    world_ref_id: i64,
    memo: String,
    favorite: bool,
) -> CmdResult<()> {
    with_library(&state, |l| l.set_world_note(world_ref_id, &memo, favorite, now()))
}

// ---- Timeline / stats ----

#[tauri::command]
pub async fn timeline_day(state: State<'_, AppState>, day: String) -> CmdResult<TimelineDay> {
    let day = require_date(&day)?;
    let (hour, reunion, first) = {
        let s = state.settings.lock().unwrap();
        (s.day_start_hour, s.reunion_days, s.show_first_met)
    };
    with_library(&state, |l| l.timeline_day(day, hour, reunion, first))
}

#[tauri::command]
pub async fn activity(state: State<'_, AppState>, from: String, to: String) -> CmdResult<Vec<DayActivity>> {
    let (from, to) = (require_date(&from)?, require_date(&to)?);
    let hour = day_start_hour(&state);
    with_library(&state, |l| l.activity(from, to, hour))
}

#[tauri::command]
pub async fn adjacent_active_day(
    state: State<'_, AppState>,
    day: String,
    forward: bool,
) -> CmdResult<Option<String>> {
    let day = require_date(&day)?;
    let hour = day_start_hour(&state);
    with_library(&state, |l| l.adjacent_active_day(day, hour, forward))
}

#[tauri::command]
pub async fn latest_active_day(state: State<'_, AppState>) -> CmdResult<Option<String>> {
    let hour = day_start_hour(&state);
    with_library(&state, |l| l.latest_active_day(hour))
}

#[tauri::command]
pub async fn on_this_day(state: State<'_, AppState>, day: String) -> CmdResult<Vec<MemoryYear>> {
    let day = require_date(&day)?;
    let hour = day_start_hour(&state);
    with_library(&state, |l| l.on_this_day(day, hour))
}

#[tauri::command]
pub async fn stats(
    state: State<'_, AppState>,
    from: Option<String>,
    to: Option<String>,
) -> CmdResult<StatsSummary> {
    let (from, to) = (parse_date(from)?, parse_date(to)?);
    let hour = day_start_hour(&state);
    with_library(&state, |l| l.stats(from, to, hour, 20))
}

#[tauri::command]
pub async fn years(state: State<'_, AppState>) -> CmdResult<Vec<i32>> {
    with_library(&state, |l| l.years())
}

// ---- Albums ----

#[tauri::command]
pub async fn albums(state: State<'_, AppState>) -> CmdResult<Vec<AlbumRow>> {
    with_library(&state, |l| l.albums())
}

#[tauri::command]
pub async fn album(state: State<'_, AppState>, id: i64) -> CmdResult<Option<AlbumRow>> {
    with_library(&state, |l| l.album(id))
}

#[tauri::command]
pub async fn create_album(state: State<'_, AppState>, name: String, description: String) -> CmdResult<AlbumRow> {
    with_library(&state, |l| l.create_album(&name, &description, now()))
}

#[tauri::command]
pub async fn update_album(
    state: State<'_, AppState>,
    id: i64,
    name: String,
    description: String,
    cover_photo_id: Option<i64>,
) -> CmdResult<()> {
    with_library(&state, |l| l.update_album(id, &name, &description, cover_photo_id))
}

#[tauri::command]
pub async fn delete_album(state: State<'_, AppState>, id: i64) -> CmdResult<()> {
    with_library(&state, |l| l.delete_album(id))
}

#[tauri::command]
pub async fn album_add_photos(
    state: State<'_, AppState>,
    album_id: i64,
    photo_ids: Vec<i64>,
) -> CmdResult<usize> {
    with_library(&state, |l| l.add_photos(album_id, &photo_ids))
}

#[tauri::command]
pub async fn album_remove_photos(
    state: State<'_, AppState>,
    album_id: i64,
    photo_ids: Vec<i64>,
) -> CmdResult<()> {
    with_library(&state, |l| l.remove_photos(album_id, &photo_ids))
}

#[tauri::command]
pub async fn album_reorder(state: State<'_, AppState>, album_id: i64, photo_ids: Vec<i64>) -> CmdResult<()> {
    with_library(&state, |l| l.reorder(album_id, &photo_ids))
}

#[tauri::command]
pub async fn album_photos(state: State<'_, AppState>, album_id: i64) -> CmdResult<Vec<PhotoRow>> {
    with_library(&state, |l| l.album_photos(album_id))
}

#[tauri::command]
pub async fn albums_of_photo(state: State<'_, AppState>, photo_id: i64) -> CmdResult<Vec<i64>> {
    with_library(&state, |l| l.albums_of_photo(photo_id))
}

#[tauri::command]
pub async fn create_album_from_visit(
    state: State<'_, AppState>,
    visit_id: i64,
    name: String,
) -> CmdResult<AlbumRow> {
    let guard = state.query.lock().unwrap();
    let conn = guard.as_ref().ok_or_else(|| NO_DB.to_string())?;
    let ids: Vec<i64> = PhotoService::new(conn)
        .photos_for_visit(visit_id)
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|p| p.id)
        .collect();
    if ids.is_empty() {
        return Err("このセッションには写真がありません".into());
    }
    let lib = LibraryService::new(conn);
    let album = lib.create_album(&name, "", now()).map_err(|e| e.to_string())?;
    lib.add_photos(album.id, &ids).map_err(|e| e.to_string())?;
    // Re-read so the returned row carries the photo count and cover.
    Ok(lib.album(album.id).map_err(|e| e.to_string())?.unwrap_or(album))
}

// ---- Story export ----

/// Replaces characters Windows forbids in file names; falls back to "album" for an empty result.
fn sanitize_file_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| if c.is_control() || "\\/:*?\"<>|".contains(c) { '_' } else { c })
        .collect();
    let cleaned = cleaned.trim().trim_end_matches('.').trim().to_string();
    if cleaned.is_empty() {
        return "album".to_string();
    }
    let stem = cleaned.split('.').next().unwrap_or("").to_ascii_uppercase();
    let numbered = stem.len() == 4
        && (stem.starts_with("COM") || stem.starts_with("LPT"))
        && stem.ends_with(|c: char| c.is_ascii_digit());
    if matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL") || numbered {
        format!("_{cleaned}")
    } else {
        cleaned
    }
}

enum StoryTarget {
    Album(i64),
    Day(NaiveDate),
}

async fn export_story(app: AppHandle, target: StoryTarget, file_name: String, include_players: bool) {
    let (db_path, start_dir, hour) = {
        let s = app.state::<AppState>().settings.lock().unwrap().clone();
        let dir = s
            .last_viewer_export_folder
            .as_deref()
            .map(PathBuf::from)
            .filter(|p| p.is_dir())
            .unwrap_or_else(settings_dir);
        (s.resolved_db_path(), dir, s.day_start_hour)
    };
    let app_dlg = app.clone();
    let picked = tauri::async_runtime::spawn_blocking(move || {
        let mut b = app_dlg
            .dialog()
            .file()
            .set_title("HTML を書き出す")
            .add_filter("HTML ファイル", &["html"])
            .set_file_name(file_name);
        if start_dir.is_dir() {
            b = b.set_directory(start_dir);
        }
        b.blocking_save_file()
    })
    .await
    .ok()
    .flatten();
    let Some(path) = picked.and_then(|p| p.into_path().ok()) else {
        return;
    };

    if let Some(folder) = path.parent() {
        let st = app.state::<AppState>();
        let mut s = st.settings.lock().unwrap();
        s.last_viewer_export_folder = Some(folder.display().to_string());
        let _ = s.save();
    }
    app::set_status(&app, "書き出し中…");
    let out = path.clone();
    let r = tauri::async_runtime::spawn_blocking(move || -> Result<ExportSummary, String> {
        let conn = app::open_db(&db_path).map_err(|e| e.to_string())?;
        let opts = StoryOptions { include_players, ..Default::default() };
        let now = now();
        match target {
            StoryTarget::Album(id) => story::export_album(&conn, id, &out, &opts, now),
            StoryTarget::Day(day) => story::export_day(&conn, day, hour, &out, &opts, now),
        }
        .map_err(|e| e.to_string())
    })
    .await;
    match r {
        Ok(Ok(sum)) => app::set_status(&app, done_message(&path.display().to_string(), &sum)),
        Ok(Err(msg)) => app::set_status(&app, format!("書き出しエラー: {msg}")),
        Err(e) => app::set_status(&app, format!("書き出しエラー: {e}")),
    }
}

fn done_message(path: &str, sum: &ExportSummary) -> String {
    let skipped = if sum.photos_skipped > 0 {
        format!("、{} 枚は読めずに省略", sum.photos_skipped)
    } else {
        String::new()
    };
    format!("書き出しました: {path}（写真 {} 枚{skipped}）", sum.photos_embedded)
}

#[tauri::command]
pub async fn export_album_html(
    app: AppHandle,
    state: State<'_, AppState>,
    album_id: i64,
    include_players: bool,
) -> CmdResult<()> {
    let album = with_library(&state, |l| l.album(album_id))?.ok_or_else(|| "アルバムが見つかりません".to_string())?;
    let file_name = format!("{}.html", sanitize_file_name(&album.name));
    export_story(app, StoryTarget::Album(album_id), file_name, include_players).await;
    Ok(())
}

#[tauri::command]
pub async fn export_day_html(app: AppHandle, day: String, include_players: bool) -> CmdResult<()> {
    let date = require_date(&day)?;
    let file_name = format!("vrclog-{}.html", date.format("%Y-%m-%d"));
    export_story(app, StoryTarget::Day(date), file_name, include_players).await;
    Ok(())
}

// ---- Links ----

const LINK_ERR: &str = "リンクを作れません";

fn is_hex_id(s: &str, prefix: &str) -> bool {
    s.strip_prefix(prefix)
        .is_some_and(|rest| rest.len() == 36 && rest.chars().all(|c| c.is_ascii_hexdigit() || c == '-'))
}

fn valid_world_id(s: &str) -> bool {
    is_hex_id(s, "wrld_")
}

fn valid_user_id(s: &str) -> bool {
    is_hex_id(s, "usr_") || (s.len() == 10 && s.chars().all(|c| c.is_ascii_alphanumeric()))
}

fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

pub(crate) fn world_url(world_id: &str) -> CmdResult<String> {
    if !valid_world_id(world_id) {
        return Err(LINK_ERR.into());
    }
    Ok(format!("https://vrchat.com/home/world/{world_id}"))
}

pub(crate) fn user_url(user_id: &str) -> CmdResult<String> {
    if !valid_user_id(user_id) {
        return Err(LINK_ERR.into());
    }
    Ok(format!("https://vrchat.com/home/user/{user_id}"))
}

pub(crate) fn instance_url(location: &str) -> CmdResult<String> {
    let (world, instance) = location.split_once(':').ok_or_else(|| LINK_ERR.to_string())?;
    if !valid_world_id(world)
        || instance.is_empty()
        || instance.chars().any(|c| c.is_whitespace() || c == '&' || c == '#')
    {
        return Err(LINK_ERR.into());
    }
    Ok(format!("https://vrchat.com/home/launch?worldId={world}&instanceId={}", percent_encode(instance)))
}

fn open(app: &AppHandle, url: String) -> CmdResult<()> {
    app.opener().open_url(url, None::<&str>).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn open_world_page(app: AppHandle, world_id: String) -> CmdResult<()> {
    open(&app, world_url(&world_id)?)
}

#[tauri::command]
pub async fn open_user_page(app: AppHandle, user_id: String) -> CmdResult<()> {
    open(&app, user_url(&user_id)?)
}

#[tauri::command]
pub async fn open_instance(app: AppHandle, location: String) -> CmdResult<()> {
    open(&app, instance_url(&location)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    const W: &str = "wrld_12345678-1234-1234-1234-123456789abc";

    #[test]
    fn world_urls() {
        assert_eq!(world_url(W).unwrap(), format!("https://vrchat.com/home/world/{W}"));
        assert!(world_url("wrld_xyz").is_err());
        assert!(world_url("usr_12345678-1234-1234-1234-123456789abc").is_err());
        assert!(world_url(&format!("{W}/../x")).is_err());
    }

    #[test]
    fn user_urls() {
        let u = "usr_12345678-1234-1234-1234-123456789abc";
        assert_eq!(user_url(u).unwrap(), format!("https://vrchat.com/home/user/{u}"));
        assert!(user_url("AbCdEf1234").is_ok());
        assert!(user_url("short").is_err());
        assert!(user_url("AbCdEf12345").is_err());
        assert!(user_url("usr_zzzz").is_err());
    }

    #[test]
    fn instance_urls() {
        let url = instance_url(&format!("{W}:12345~region(jp)")).unwrap();
        assert_eq!(url, format!("https://vrchat.com/home/launch?worldId={W}&instanceId=12345~region%28jp%29"));
        assert!(instance_url(&format!("{W}:")).is_err());
        assert!(instance_url(W).is_err());
        assert!(instance_url(&format!("{W}:1 2")).is_err());
        assert!(instance_url(&format!("{W}:1&x=2")).is_err());
        assert!(instance_url(&format!("{W}:1#a")).is_err());
        assert!(instance_url("wrld_bad:1").is_err());
    }

    #[test]
    fn file_names() {
        assert_eq!(sanitize_file_name("a/b:c*?"), "a_b_c__");
        assert_eq!(sanitize_file_name("  "), "album");
        assert_eq!(sanitize_file_name("CON"), "_CON");
        assert_eq!(sanitize_file_name("旅の記録"), "旅の記録");
    }

    #[test]
    fn badges_hide_first_met() {
        let b = |id, first, days| PlayerBadge { player_id: id, first_met: first, days_since_last: days };
        let out = filter_badges(vec![b(1, true, None), b(2, true, Some(200)), b(3, false, Some(5))], false);
        assert_eq!(out.len(), 2);
        assert!(out.iter().all(|x| !x.first_met));
        assert_eq!(filter_badges(vec![b(1, true, None)], true).len(), 1);
    }

    #[test]
    fn done_messages() {
        let s = ExportSummary { photos_embedded: 3, photos_skipped: 0 };
        assert_eq!(done_message("x.html", &s), "書き出しました: x.html（写真 3 枚）");
        let s = ExportSummary { photos_embedded: 3, photos_skipped: 2 };
        assert!(done_message("x.html", &s).contains("2 枚は読めずに省略"));
    }
}
