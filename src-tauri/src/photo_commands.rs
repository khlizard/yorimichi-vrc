//! Tauri commands for the photo library: listing, details, favorites, opening, clipboard.
//! JS argument names are camelCase.

use chrono::Local;
use tauri::{AppHandle, State};
use tauri_plugin_opener::OpenerExt;
use vrclogger_core::data::photo_query::{DayCount, PhotoDetail, PhotoFilter, PhotoRow, PhotoService};

use crate::app::{self, AppState};
use crate::commands::{keyset, parse_date, write_clipboard, CmdResult};

fn with_photos<T>(
    state: &AppState,
    f: impl FnOnce(&PhotoService) -> vrclogger_core::Result<T>,
) -> CmdResult<T> {
    app::with_query_conn(state, |conn| f(&PhotoService::new(conn)))
}

#[tauri::command]
pub async fn rescan_photos(app: AppHandle) -> CmdResult<()> {
    app::run_import(app, false).await;
    Ok(())
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn photos(
    state: State<'_, AppState>,
    from: Option<String>,
    to: Option<String>,
    world_ref_id: Option<i64>,
    player_id: Option<i64>,
    visit_id: Option<i64>,
    favorite_only: bool,
    before_time: Option<String>,
    before_id: Option<i64>,
    limit: usize,
) -> CmdResult<Vec<PhotoRow>> {
    let filter = PhotoFilter {
        from: parse_date(from)?,
        to: parse_date(to)?,
        world_ref_id,
        player_id,
        visit_id,
        favorite_only,
        include_missing: false,
    };
    let before = keyset(before_time, before_id);
    with_photos(&state, |p| p.photos(&filter, before, limit.clamp(1, 500)))
}

#[tauri::command]
pub async fn photo_days(
    state: State<'_, AppState>,
    from: Option<String>,
    to: Option<String>,
    world_ref_id: Option<i64>,
    player_id: Option<i64>,
    favorite_only: bool,
) -> CmdResult<Vec<DayCount>> {
    let filter = PhotoFilter {
        from: parse_date(from)?,
        to: parse_date(to)?,
        world_ref_id,
        player_id,
        favorite_only,
        ..Default::default()
    };
    let hour = state.settings.lock().unwrap().day_start_hour;
    with_photos(&state, |p| p.photo_days(&filter, hour))
}

#[tauri::command]
pub async fn photos_for_visit(state: State<'_, AppState>, visit_id: i64) -> CmdResult<Vec<PhotoRow>> {
    with_photos(&state, |p| p.photos_for_visit(visit_id))
}

#[tauri::command]
pub async fn photo_detail(state: State<'_, AppState>, id: i64) -> CmdResult<Option<PhotoDetail>> {
    with_photos(&state, |p| p.photo_detail(id))
}

#[tauri::command]
pub async fn set_photo_favorite(state: State<'_, AppState>, id: i64, favorite: bool) -> CmdResult<()> {
    with_photos(&state, |p| p.set_favorite(id, favorite))
}

fn photo_path_of(state: &AppState, id: i64) -> CmdResult<String> {
    with_photos(state, |p| p.photo_path(id))?.ok_or_else(|| "ファイルが見つかりません".to_string())
}

#[tauri::command]
pub async fn reveal_photo(app: AppHandle, state: State<'_, AppState>, id: i64) -> CmdResult<()> {
    let path = photo_path_of(&state, id)?;
    app.opener().reveal_item_in_dir(path).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn open_photo(app: AppHandle, state: State<'_, AppState>, id: i64) -> CmdResult<()> {
    let path = photo_path_of(&state, id)?;
    app.opener().open_path(path, None::<&str>).map_err(|e| e.to_string())
}

/// File facts for the photo info dialog. Only attributes are read (never the content).
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PhotoFileInfo {
    pub path: String,
    pub folder: String,
    pub exists: bool,
    pub online_only: bool,
    pub size: Option<u64>,
    /// Local time `YYYY-MM-DD HH:MM:SS`.
    pub modified: Option<String>,
    pub read_only: bool,
}

#[tauri::command]
pub async fn photo_file_info(state: State<'_, AppState>, id: i64) -> CmdResult<PhotoFileInfo> {
    let path = photo_path_of(&state, id)?;
    let p = std::path::Path::new(&path);
    let folder = p.parent().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default();
    Ok(match std::fs::metadata(p) {
        Ok(md) => PhotoFileInfo {
            online_only: vrclogger_core::photos::is_online_only(&md),
            size: Some(md.len()),
            modified: md
                .modified()
                .ok()
                .map(|t| chrono::DateTime::<Local>::from(t).format(vrclogger_core::data::db::TS_FORMAT).to_string()),
            read_only: md.permissions().readonly(),
            exists: true,
            path,
            folder,
        },
        Err(_) => PhotoFileInfo { path, folder, exists: false, online_only: false, size: None, modified: None, read_only: false },
    })
}

/// Copies the full-size image to the clipboard. Online-only files are refused (no download).
#[tauri::command]
pub async fn copy_photo_image(state: State<'_, AppState>, id: i64) -> CmdResult<()> {
    let path = photo_path_of(&state, id)?;
    let r = tauri::async_runtime::spawn_blocking(move || {
        let p = std::path::Path::new(&path);
        let md = std::fs::metadata(p).map_err(|_| "ファイルが見つかりません".to_string())?;
        if vrclogger_core::photos::is_online_only(&md) {
            return Err("オンライン専用のファイルはコピーできません（ダウンロードされていません）。".to_string());
        }
        let (width, height, bytes) =
            vrclogger_core::photos::rgba_pixels(p).map_err(|e| format!("画像を読み込めませんでした: {e}"))?;
        // Borrowed pixels: `set_image` takes the image by value, so each attempt clones only this
        // borrowing struct (a reference), never the pixel buffer (about 33 MB at 4K).
        let image = arboard::ImageData {
            width: width as usize,
            height: height as usize,
            bytes: std::borrow::Cow::Borrowed(bytes.as_slice()),
        };
        write_clipboard(|c| c.set_image(image.clone()))
    })
    .await;
    app::flatten_join(r)
}
