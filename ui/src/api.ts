// The contract between the React UI and the Rust backend (src-tauri).
// All commands return Promises that reject with a string (Japanese message) on failure.
// Dates are "yyyy-MM-dd" strings or null (inclusive on both ends).
// Timestamps in DTOs are local-time strings "yyyy-MM-dd HH:mm:ss".
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

/** Page size the UI should use for the player list (recentPlayers / searchPlayers). */
export const PLAYER_PAGE_SIZE = 100;
/** Page size the UI should use for the raw log list (searchRawEvents). */
export const RAW_PAGE_SIZE = 200;
/** Sentinel the UI may show for "no filter" in kind/level selects; the backend treats it (and "") as none. */
export const ALL = "(全て)";

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
}

export interface VisitPlayerRow {
  playerId: number;
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

// ---- Settings (PascalCase keys, as stored in %APPDATA%\VrcLogger\vrclogger.settings.json) ----

export interface Settings {
  DbPath: string;
  LogFolders: string[];
  AutoImportIntervalMinutes: number;
  MinimizeToTray: boolean;
  RawLogRetentionDays: number;
  RawLogNoisePatterns: string[];
  LastViewerExportFolder: string | null;
}

export interface Status {
  status: string;
  importing: boolean;
  /** Resolved absolute DB path. */
  dbPath: string;
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
/** Opens the save dialog; cancel does nothing. Result is reported via status events. */
export const exportViewer = () => invoke<void>("export_viewer");

export const getSettings = () => invoke<Settings>("get_settings");
/** Returns the save status text, e.g. "保存しました (12:34:56)". */
export const saveSettings = (settings: Settings, startWithWindows: boolean) =>
  invoke<string>("save_settings", { settings, startWithWindows });
export const defaultNoisePatterns = () => invoke<string[]>("default_noise_patterns");
/** Folder picker; null when cancelled. */
export const pickFolder = () => invoke<string | null>("pick_folder");
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
