//! Photo support in the shell: scan orchestration, thumbnail cache and the `vrcphoto` URI scheme.
//!
//! Photo files are only ever read. Online-only (cloud placeholder) files are never opened;
//! their thumbnails come from the shell thumbnail pipeline (see `shell_thumb`).

use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use rusqlite::{Connection, OptionalExtension};
use serde::Serialize;
use tauri::http::{header, Request, Response, StatusCode};
use tauri::{AppHandle, Emitter, Manager};
use vrclogger_core::data::photo_query::PhotoService;
use vrclogger_core::photos::{is_online_only, make_thumbnail, relink_visits, scan_folders};

use crate::app::{set_status, AppState};
use crate::shell_thumb::shell_thumbnail_jpeg;

pub const SCAN_INTERVAL: Duration = Duration::from_secs(300);
const THUMB_EDGE: u32 = 400;
const PREGEN_MAX: usize = 2000;
const PREGEN_PAGE: usize = 200;
const PREGEN_DELAY: Duration = Duration::from_secs(20);

#[derive(Serialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct PhotosChanged {
    pub added: usize,
    pub updated: usize,
    pub missing: usize,
    pub relinked: usize,
}

impl PhotosChanged {
    pub fn any(&self) -> bool {
        self.added + self.updated + self.missing + self.relinked > 0
    }
}

/// Runs the scan / relink step of an import on the import connection (blocking).
/// Errors are reported through the status text and never fail the import.
pub fn scan_step(app: &AppHandle, conn: &Connection, folders: &[PathBuf], do_scan: bool, visits_changed: bool) -> PhotosChanged {
    let mut out = PhotosChanged::default();
    if folders.is_empty() {
        return out;
    }
    if do_scan {
        let progress = |m: &str| set_status(app, m);
        match scan_folders(conn, folders, Some(&progress as &dyn Fn(&str))) {
            Ok(s) => {
                out.added = s.added;
                out.updated = s.updated;
                out.missing = s.missing;
                if s.added + s.updated > 0 {
                    set_status(app, format!("写真: {} 件追加 / {} 件更新", s.added, s.updated));
                }
            }
            Err(e) => set_status(app, format!("写真の読み込みエラー: {e}")),
        }
        *app.state::<AppState>().next_photo_scan_at.lock().unwrap() = Some(Instant::now() + SCAN_INTERVAL);
    } else if visits_changed {
        match relink_visits(conn) {
            Ok(n) => out.relinked = n,
            Err(e) => set_status(app, format!("写真の読み込みエラー: {e}")),
        }
    }
    out
}

pub fn scan_due(app: &AppHandle, automatic: bool) -> bool {
    if !automatic {
        return true;
    }
    app.state::<AppState>().next_photo_scan_at.lock().unwrap().is_none_or(|t| Instant::now() >= t)
}

pub fn emit_changed(app: &AppHandle, ch: &PhotosChanged) {
    if ch.any() {
        let _ = app.emit("photos-changed", ch.clone());
    }
    if ch.added > 0 {
        start_pregen(app);
    }
}

// ---- Thumbnail cache ----

/// FNV-1a 64-bit hash.
fn fnv1a64(data: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in data {
        h ^= *b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

pub fn thumb_key(path: &str, file_size: i64, mtime: &str) -> String {
    format!("{:016x}", fnv1a64(format!("{path}|{file_size}|{mtime}").as_bytes()))
}

#[derive(Debug, Clone)]
pub struct PhotoFile {
    pub path: String,
    pub missing: bool,
    pub online_only: bool,
    pub file_size: i64,
    pub mtime: String,
}

impl PhotoFile {
    fn key(&self) -> String {
        thumb_key(&self.path, self.file_size, &self.mtime)
    }
    /// The DB flag dates from the last scan; the sync client may have turned the file into a
    /// placeholder since, so the attribute is checked again right before any read.
    fn servable(&self) -> bool {
        !self.missing
            && !self.online_only
            && std::fs::metadata(&self.path).is_ok_and(|md| md.is_file() && !is_online_only(&md))
    }
}

impl PhotoFile {
    /// Not missing and online-only, per the DB flag or the current attributes.
    fn online_only_now(&self) -> bool {
        if self.missing {
            return false;
        }
        match std::fs::metadata(&self.path) {
            Ok(md) => md.is_file() && (self.online_only || is_online_only(&md)),
            Err(_) => false,
        }
    }
}

/// Gets a thumbnail for an online-only file from the shell and caches it (blocking; caller holds the gate).
/// If the call made the file local (hydrated it), the shell path is disabled for the session.
fn ensure_shell_thumb(app: &AppHandle, file: &PhotoFile, cache: &Path) -> Option<Vec<u8>> {
    let st = app.state::<AppState>();
    if st.shell_thumb_disabled.load(Ordering::SeqCst) {
        return None;
    }
    let key = file.key();
    if st.thumb_failed.lock().unwrap().contains(&key) {
        return None;
    }
    let bytes = shell_thumbnail_jpeg(Path::new(&file.path), THUMB_EDGE);
    let still_online_only = std::fs::metadata(&file.path).is_ok_and(|md| is_online_only(&md));
    if !still_online_only {
        st.shell_thumb_disabled.store(true, Ordering::SeqCst);
        set_status(app, "オンライン専用の写真のサムネイル取得を停止しました (ファイルのダウンロードが発生したため)");
    }
    let Some(bytes) = bytes else {
        st.thumb_failed.lock().unwrap().insert(key);
        return None;
    };
    if let Some(dir) = cache.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let tmp = cache.with_extension("jpg.tmp");
    if std::fs::write(&tmp, &bytes).is_ok() && std::fs::rename(&tmp, cache).is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    Some(bytes)
}

pub fn photo_file(conn: &Connection, id: i64) -> rusqlite::Result<Option<PhotoFile>> {
    conn.query_row(
        "SELECT path, missing, online_only, file_size, mtime FROM photos WHERE id = ?1",
        [id],
        |r| {
            Ok(PhotoFile {
                path: r.get(0)?,
                missing: r.get(1)?,
                online_only: r.get(2)?,
                file_size: r.get(3)?,
                mtime: r.get(4)?,
            })
        },
    )
    .optional()
}

fn lookup(app: &AppHandle, id: i64) -> Option<PhotoFile> {
    let st = app.state::<AppState>();
    let guard = st.query.lock().unwrap();
    photo_file(guard.as_ref()?, id).ok().flatten()
}

fn thumbs_dir(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_local_data_dir().ok().map(|d| d.join("thumbs"))
}

/// Returns the cached thumbnail path, generating it when needed (blocking; caller bounds concurrency).
fn ensure_thumb(app: &AppHandle, file: &PhotoFile) -> Option<PathBuf> {
    let key = file.key();
    let cache = thumbs_dir(app)?.join(format!("{key}.jpg"));
    if cache.is_file() {
        return Some(cache);
    }
    if !file.servable() {
        return None;
    }
    let st = app.state::<AppState>();
    if st.thumb_failed.lock().unwrap().contains(&key) {
        return None;
    }
    let result = make_thumbnail(Path::new(&file.path), &cache, THUMB_EDGE);
    // A concurrent generation of the same key may have produced the file even if our rename lost.
    if result.is_ok() || cache.is_file() {
        Some(cache)
    } else {
        st.thumb_failed.lock().unwrap().insert(key);
        None
    }
}

/// Background pre-generation of thumbnails for the newest photos; a newer call supersedes the running one.
pub fn start_pregen(app: &AppHandle) {
    let generation = app.state::<AppState>().thumb_generation.fetch_add(1, Ordering::SeqCst) + 1;
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let st = app.state::<AppState>();
        let stop = || st.exit_requested.load(Ordering::SeqCst) || st.thumb_generation.load(Ordering::SeqCst) != generation;
        let mut offset = 0;
        while offset < PREGEN_MAX {
            let page = {
                let guard = st.query.lock().unwrap();
                let Some(conn) = guard.as_ref() else { return };
                match PhotoService::new(conn).photo_ids_newest(PREGEN_PAGE, offset) {
                    Ok(p) => p,
                    Err(_) => return,
                }
            };
            if page.is_empty() {
                return;
            }
            for (id, _, online_only) in &page {
                if stop() {
                    return;
                }
                if *online_only {
                    continue;
                }
                let Some(file) = lookup(&app, *id) else { continue };
                if !file.servable() {
                    continue;
                }
                let cached = thumbs_dir(&app).is_some_and(|d| d.join(format!("{}.jpg", file.key())).is_file());
                if cached {
                    continue;
                }
                let _permit = tauri::async_runtime::block_on(st.thumb_gate.acquire());
                ensure_thumb(&app, &file);
                drop(_permit);
                std::thread::sleep(Duration::from_millis(30));
            }
            offset += PREGEN_PAGE;
        }
    });
}

pub fn start_pregen_delayed(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(PREGEN_DELAY).await;
        start_pregen(&app);
    });
}

// ---- vrcphoto:// scheme ----

#[derive(Debug, PartialEq, Eq)]
pub enum PhotoRequest {
    Thumb(i64),
    Full(i64),
}

fn percent_decode(s: &str) -> Option<String> {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            let hex = s.get(i + 1..i + 3)?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

/// Parses a request path such as `/thumb%2F12` or `/full/7`.
pub fn parse_photo_path(path: &str) -> Result<PhotoRequest, String> {
    let decoded = percent_decode(path).ok_or("bad encoding")?;
    let rest = decoded.trim_start_matches('/');
    let (kind, id) = rest.split_once('/').ok_or("bad path")?;
    let id: i64 = id.parse().map_err(|_| "bad id".to_string())?;
    match kind {
        "thumb" => Ok(PhotoRequest::Thumb(id)),
        "full" => Ok(PhotoRequest::Full(id)),
        _ => Err("bad kind".into()),
    }
}

fn respond(status: StatusCode, content_type: Option<&str>, cache: Option<&str>, body: Vec<u8>) -> Response<Vec<u8>> {
    let mut b = Response::builder().status(status).header("Access-Control-Allow-Origin", "*");
    if let Some(ct) = content_type {
        b = b.header(header::CONTENT_TYPE, ct);
    }
    if let Some(c) = cache {
        b = b.header(header::CACHE_CONTROL, c);
    }
    b.body(body).unwrap_or_else(|_| Response::new(Vec::new()))
}

fn empty(status: StatusCode) -> Response<Vec<u8>> {
    respond(status, None, None, Vec::new())
}

fn content_type_for(path: &str) -> &'static str {
    match Path::new(path).extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).as_deref() {
        Some("png") => "image/png",
        Some("webp") => "image/webp",
        _ => "image/jpeg",
    }
}

pub async fn handle_request(app: AppHandle, request: Request<Vec<u8>>) -> Response<Vec<u8>> {
    let Ok(req) = parse_photo_path(request.uri().path()) else {
        return empty(StatusCode::BAD_REQUEST);
    };
    match req {
        PhotoRequest::Thumb(id) => {
            let Some(file) = lookup(&app, id) else { return empty(StatusCode::NOT_FOUND) };
            let app2 = app.clone();
            let bytes = tauri::async_runtime::spawn_blocking(move || {
                let cache = thumbs_dir(&app2)?.join(format!("{}.jpg", file.key()));
                if let Ok(b) = std::fs::read(&cache) {
                    return Some(b);
                }
                let st = app2.state::<AppState>();
                if file.online_only_now() {
                    let _permit = tauri::async_runtime::block_on(st.thumb_gate.acquire()).ok()?;
                    // Another request may have filled the cache while waiting.
                    if let Ok(b) = std::fs::read(&cache) {
                        return Some(b);
                    }
                    return ensure_shell_thumb(&app2, &file, &cache);
                }
                if !file.servable() {
                    return None;
                }
                let _permit = tauri::async_runtime::block_on(st.thumb_gate.acquire()).ok()?;
                let path = ensure_thumb(&app2, &file)?;
                std::fs::read(path).ok()
            })
            .await
            .ok()
            .flatten();
            match bytes {
                Some(b) => respond(StatusCode::OK, Some("image/jpeg"), Some("private, max-age=86400"), b),
                None => empty(StatusCode::NOT_FOUND),
            }
        }
        PhotoRequest::Full(id) => {
            let Some(file) = lookup(&app, id) else { return empty(StatusCode::NOT_FOUND) };
            if !file.servable() {
                return empty(StatusCode::NOT_FOUND);
            }
            let ct = content_type_for(&file.path);
            let bytes = tauri::async_runtime::spawn_blocking(move || std::fs::read(&file.path).ok())
                .await
                .ok()
                .flatten();
            match bytes {
                Some(b) => respond(StatusCode::OK, Some(ct), Some("private, max-age=3600"), b),
                None => empty(StatusCode::NOT_FOUND),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_is_stable_and_sensitive() {
        let a = thumb_key("C:\\p\\a.png", 100, "2026-01-01 00:00:00");
        assert_eq!(a, thumb_key("C:\\p\\a.png", 100, "2026-01-01 00:00:00"));
        assert_eq!(a.len(), 16);
        assert_ne!(a, thumb_key("C:\\p\\a.png", 101, "2026-01-01 00:00:00"));
        assert_ne!(a, thumb_key("C:\\p\\a.png", 100, "2026-01-01 00:00:01"));
        // Known FNV-1a 64 vector.
        assert_eq!(fnv1a64(b"a"), 0xaf63_dc4c_8601_ec8c);
    }

    #[test]
    fn parses_paths() {
        assert_eq!(parse_photo_path("/thumb%2F12"), Ok(PhotoRequest::Thumb(12)));
        assert_eq!(parse_photo_path("/full/7"), Ok(PhotoRequest::Full(7)));
        assert!(parse_photo_path("/thumb/abc").is_err());
        assert!(parse_photo_path("/other/1").is_err());
        assert!(parse_photo_path("/thumb").is_err());
        assert!(parse_photo_path("/thumb%2").is_err());
    }
}
