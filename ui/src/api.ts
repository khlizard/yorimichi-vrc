// The contract between the React UI and the Rust backend (src-tauri).
// All commands return Promises that reject with a string (Japanese message) on failure.
// Dates are "yyyy-MM-dd" strings or null (inclusive on both ends).
// Timestamps in DTOs are local-time strings "yyyy-MM-dd HH:mm:ss".
import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

/** Page size the UI should use for the player list (recentPlayers / searchPlayers). */
export const PLAYER_PAGE_SIZE = 100;
/** Page size the UI should use for the raw log list (searchRawEvents). */
export const RAW_PAGE_SIZE = 200;
/** Sentinel the UI may show for "no filter" in kind/level selects; the backend treats it (and "") as none. */
export const ALL = "(全て)";
/** Page size the UI should use for the photo grid (photos). */
export const PHOTO_PAGE_SIZE = 200;

// ---- DTOs (camelCase, mirrors vrclogger-core data/dtos.rs) ----

export interface PlayerHit {
  id: number;
  userId: string;
  displayName: string;
  isLocal: boolean;
  otherNames: string | null;
  encounterCount: number;
  lastSeen: string | null;
  /** yyyy-MM-dd part of lastSeen, or "" */
  lastSeenDate: string;
}

export interface EncounterRow {
  visitId: number;
  displayName: string;
  worldName: string;
  instanceId: string;
  joinTime: string;
  leaveTime: string;
  endedCleanly: boolean;
  avatarName: string | null;
  avatarSizeMb: number | null;
  duration: string;
}

export interface VisitRow {
  id: number;
  worldName: string;
  instanceId: string;
  location: string;
  joinTime: string;
  leaveTime: string;
  endedCleanly: boolean;
  playerCount: number;
  duration: string;
  worldId: string | null;
  worldRefId: number | null;
}

export interface VisitPlayerRow {
  playerId: number;
  userId: string;
  displayName: string;
  isLocal: boolean;
  joinTime: string;
  leaveTime: string;
  endedCleanly: boolean;
  avatarName: string | null;
  avatarSizeMb: number | null;
  duration: string;
}

export interface AvatarRow {
  visitId: number | null;
  eventTime: string;
  displayName: string;
  avatarName: string;
  sizeMb: number | null;
  loadSeconds: number | null;
  worldName: string;
}

export interface UrlHit {
  visitId: number | null;
  eventTime: string;
  kind: string;
  url: string;
  worldName: string;
}

export interface RawEventHit {
  id: number;
  visitId: number | null;
  eventTime: string;
  level: string;
  message: string;
  worldName: string;
}

export interface NotificationRow {
  id: number;
  notificationId: string;
  eventTime: string;
  kind: string;
  senderUserId: string;
  senderName: string;
  message: string;
  details: string;
  acceptedAt: string | null;
  response: string;
}

export interface ModerationRow {
  visitId: number | null;
  eventTime: string;
  kind: string;
  targetName: string;
  worldName: string;
}

export interface PhotoRow {
  id: number;
  path: string;
  fileName: string;
  /** "yyyy-MM-dd HH:mm:ss" */
  takenAt: string;
  timeSource: string;
  width: number | null;
  height: number | null;
  favorite: boolean;
  missing: boolean;
  /** Cloud placeholder: no thumbnail / full image is available. */
  onlineOnly: boolean;
  visitId: number | null;
  worldName: string | null;
  worldRefId: number | null;
}

export interface PhotoPlayer {
  playerId: number;
  userId: string;
  displayName: string;
  isLocal: boolean;
}

export interface PhotoDetail {
  photo: PhotoRow;
  worldId: string | null;
  instanceId: string | null;
  location: string | null;
  visitJoinTime: string | null;
  visitLeaveTime: string | null;
  authorId: string | null;
  players: PhotoPlayer[];
}

export interface DayCount {
  /** yyyy-MM-dd (shifted by the day-start-hour setting) */
  day: string;
  count: number;
}

/** Payload of the "photos-changed" event. */
export interface PhotosChanged {
  added: number;
  updated: number;
  missing: number;
  relinked: number;
}

// ---- Settings (PascalCase keys, as stored in %APPDATA%\VrcLogger\vrclogger.settings.json) ----

export interface Settings {
  DbPath: string;
  LogFolders: string[];
  AutoImportIntervalMinutes: number;
  MinimizeToTray: boolean;
  RawLogRetentionDays: number;
  RawLogNoisePatterns: string[];
  LastViewerExportFolder: string | null;
  /** false until the user has answered the "stay in the tray?" question. */
  TrayChoiceMade: boolean;
  PhotoFolders: string[];
  /** 0-23: times before this hour count as the previous day. */
  DayStartHour: number;
  ShowFirstMet: boolean;
  /** 0 = no badge. */
  ReunionDays: number;
  AutoVacuum: boolean;
  AutoVacuumMinDbMb: number;
  /** Relative paths are resolved against the settings folder. */
  BackupFolder: string;
  /** 0 = no automatic backup. */
  BackupIntervalDays: number;
  BackupKeepCount: number;
  /** Backend-owned, "yyyy-MM-dd HH:mm:ss" local time. */
  LastBackupAt: string | null;
  /** http(s) URL or folder; empty = never check. */
  UpdateFeed: string;
  /** Display theme. */
  Theme: "auto" | "light" | "dark";
}

export interface Status {
  status: string;
  importing: boolean;
  /** Resolved absolute DB path. */
  dbPath: string;
  dbSizeBytes: number;
  /** Bytes a VACUUM would give back. */
  reclaimableBytes: number;
  /** Big enough and mostly free pages: worth running VACUUM. */
  vacuumRecommended: boolean;
  version: string;
}

export interface UpdateReady {
  version: string;
}

export interface ImportFinished {
  /** true when new data was imported, rows were pruned, or an error occurred (reload lists). */
  changed: boolean;
  /** true when triggered by the timer (avoid disturbing scrolled lists), false when manual. */
  automatic: boolean;
}

// ---- Commands ----

export const getStatus = () => invoke<Status>("get_status");
/** Manual import. Resolves when done (immediately if an import is already running). */
export const importNow = () => invoke<void>("import_now");
/** Marks all files for reimport and runs a manual import. */
export const reimportAll = () => invoke<void>("reimport_all");
export const vacuum = () => invoke<void>("vacuum");

export const getSettings = () => invoke<Settings>("get_settings");
/** Returns the save status text, e.g. "保存しました (12:34:56)". */
export const saveSettings = (settings: Settings, startWithWindows: boolean) =>
  invoke<string>("save_settings", { settings, startWithWindows });
export const defaultNoisePatterns = () => invoke<string[]>("default_noise_patterns");
/** Folder picker; null when cancelled. */
export const pickFolder = (title?: string) => invoke<string | null>("pick_folder", { title: title ?? null });
/** Answer to the "ask-tray" question: stay = keep running in the tray (hides the window), else exits the app. */
export const answerTrayChoice = (stay: boolean) => invoke<void>("answer_tray_choice", { stay });
/** Backs up the DB now; the result arrives via status events. */
export const backupNow = () => invoke<void>("backup_now");
/** Manual update check; the result arrives via status / "update-ready" events. */
export const checkUpdate = () => invoke<void>("check_update");
/** Applies the downloaded update and restarts the app. */
export const applyUpdate = () => invoke<void>("apply_update");
export const autostartGet = () => invoke<boolean>("autostart_get");
/** Clipboard copy with retries (IME lock). */
export const copyText = (text: string) => invoke<void>("copy_text", { text });
/** Opens in the default browser. */
export const openUrl = (url: string) => invoke<void>("open_url", { url });

export const recentPlayers = (from: string | null, to: string | null, offset: number, limit: number) =>
  invoke<PlayerHit[]>("recent_players", { from, to, offset, limit });
export const searchPlayers = (term: string, offset: number, limit: number) =>
  invoke<PlayerHit[]>("search_players", { term, offset, limit });
export const encounters = (playerId: number, from: string | null, to: string | null) =>
  invoke<EncounterRow[]>("encounters", { playerId, from, to });
export const visits = (worldTerm: string | null, from: string | null, to: string | null) =>
  invoke<VisitRow[]>("visits", { worldTerm, from, to });
export const visitPlayers = (visitId: number) => invoke<VisitPlayerRow[]>("visit_players", { visitId });
export const visitUrls = (visitId: number) => invoke<UrlHit[]>("visit_urls", { visitId });
/** Max 2000 rows. kind: "" / "(全て)" / null = all. */
export const searchUrls = (term: string, kind: string | null, from: string | null, to: string | null) =>
  invoke<UrlHit[]>("search_urls", { term, kind, from, to });
/**
 * Newest first. Paginate with beforeTime/beforeId = eventTime/id of the last row received
 * (both null for the first page). level: "" / "(全て)" / null = all.
 */
export const searchRawEvents = (
  term: string,
  level: string | null,
  from: string | null,
  to: string | null,
  beforeTime: string | null,
  beforeId: number | null,
  limit: number,
) => invoke<RawEventHit[]>("search_raw_events", { term, level, from, to, beforeTime, beforeId, limit });
export const searchAvatars = (term: string, from: string | null, to: string | null) =>
  invoke<AvatarRow[]>("search_avatars", { term, from, to });
export const searchNotifications = (term: string, kind: string | null, from: string | null, to: string | null) =>
  invoke<NotificationRow[]>("search_notifications", { term, kind, from, to });
export const notificationKinds = () => invoke<string[]>("notification_kinds");
export const searchModeration = (term: string, from: string | null, to: string | null) =>
  invoke<ModerationRow[]>("search_moderation", { term, from, to });

export interface PhotoQuery {
  from?: string | null;
  to?: string | null;
  worldRefId?: number | null;
  playerId?: number | null;
  visitId?: number | null;
  favoriteOnly?: boolean;
  /** Keyset paging: takenAt / id of the last row received (omit for the first page). */
  beforeTime?: string | null;
  beforeId?: number | null;
  /** Clamped to 1..500 by the backend. */
  limit: number;
}

/** Newest first. */
export const photos = (q: PhotoQuery) =>
  invoke<PhotoRow[]>("photos", {
    from: q.from ?? null,
    to: q.to ?? null,
    worldRefId: q.worldRefId ?? null,
    playerId: q.playerId ?? null,
    visitId: q.visitId ?? null,
    favoriteOnly: q.favoriteOnly ?? false,
    beforeTime: q.beforeTime ?? null,
    beforeId: q.beforeId ?? null,
    limit: q.limit,
  });
export const photoDays = (
  from: string | null,
  to: string | null,
  worldRefId: number | null,
  playerId: number | null,
  favoriteOnly: boolean,
) => invoke<DayCount[]>("photo_days", { from, to, worldRefId, playerId, favoriteOnly });
export const photosForVisit = (visitId: number) => invoke<PhotoRow[]>("photos_for_visit", { visitId });
export const photoDetail = (id: number) => invoke<PhotoDetail | null>("photo_detail", { id });
export const setPhotoFavorite = (id: number, favorite: boolean) =>
  invoke<void>("set_photo_favorite", { id, favorite });
/** Manual import + photo scan (always scans). */
export const rescanPhotos = () => invoke<void>("rescan_photos");
/** Show the file selected in Explorer. */
export const revealPhoto = (id: number) => invoke<void>("reveal_photo", { id });
/** Open with the default app. */
export const openPhoto = (id: number) => invoke<void>("open_photo", { id });

export interface PhotoFileInfo {
  path: string;
  folder: string;
  exists: boolean;
  onlineOnly: boolean;
  size: number | null;
  /** "YYYY-MM-DD HH:MM:SS" */
  modified: string | null;
  readOnly: boolean;
}
export const photoFileInfo = (id: number) => invoke<PhotoFileInfo>("photo_file_info", { id });
/** Copy the full image to the clipboard (rejects for online-only / missing files). */
export const copyPhotoImage = (id: number) => invoke<void>("copy_photo_image", { id });

/** 400px JPEG thumbnail (vrcphoto scheme; 404 for missing / online-only photos). */
export const thumbUrl = (id: number): string => convertFileSrc(`thumb/${id}`, "vrcphoto");
/** Original image file. */
export const fullUrl = (id: number): string => convertFileSrc(`full/${id}`, "vrcphoto");

// ---- Events ----

/** Every status text change (progress, errors, results). Payload: string. */
export const onStatus = (cb: (status: string) => void): Promise<UnlistenFn> =>
  listen<string>("status", (e) => cb(e.payload));
/** Emitted after each import run (manual or automatic). */
export const onImportFinished = (cb: (e: ImportFinished) => void): Promise<UnlistenFn> =>
  listen<ImportFinished>("import-finished", (e) => cb(e.payload));
/** Emitted whenever the importing flag flips (payload boolean), so the UI can disable buttons. */
export const onImporting = (cb: (importing: boolean) => void): Promise<UnlistenFn> =>
  listen<boolean>("importing", (e) => cb(e.payload));
/** Close button pressed before the tray question was answered: show the choice dialog. */
export const onAskTray = (cb: () => void): Promise<UnlistenFn> => listen("ask-tray", () => cb());
/** Settings were saved (or changed by the backend, e.g. LastBackupAt). Payload: the saved Settings. */
export const onSettingsChanged = (cb: (s: Settings) => void): Promise<UnlistenFn> =>
  listen<Settings>("settings-changed", (e) => cb(e.payload));
/** An update was downloaded and waits for a restart. */
export const onUpdateReady = (cb: (e: UpdateReady) => void): Promise<UnlistenFn> =>
  listen<UpdateReady>("update-ready", (e) => cb(e.payload));
/** The photo library changed (scan added / updated / marked missing, or visit links changed). */
export const onPhotosChanged = (cb: (e: PhotosChanged) => void): Promise<UnlistenFn> =>
  listen<PhotosChanged>("photos-changed", (e) => cb(e.payload));
