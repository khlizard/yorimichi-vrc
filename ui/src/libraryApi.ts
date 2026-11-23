import { invoke } from "@tauri-apps/api/core";
import type { PhotoRow, PlayerHit } from "./api";

export interface Tag {
  id: number;
  name: string;
  /** #rrggbb */
  color: string;
}

export interface TagCount extends Tag {
  playerCount: number;
}

export interface PlayerProfile {
  id: number;
  userId: string;
  displayName: string;
  isLocal: boolean;
  memo: string;
  tags: Tag[];
  firstMet: string | null;
  lastMet: string | null;
  encounterCount: number;
  totalSeconds: number;
  photoCount: number;
}

export interface PlayerMark {
  playerId: number;
  hasMemo: boolean;
  tags: Tag[];
}

export interface PlayerBadge {
  playerId: number;
  firstMet: boolean;
  daysSinceLast: number | null;
}

export interface WorldRow {
  id: number;
  worldId: string;
  name: string;
  visitCount: number;
  firstVisit: string | null;
  lastVisit: string | null;
  totalSeconds: number;
  memo: string;
  favorite: boolean;
  photoCount: number;
}

export type WorldSort = "lastVisit" | "visitCount" | "totalTime" | "name";

export interface TimelinePlayer {
  playerId: number;
  userId: string;
  displayName: string;
  joinTime: string;
  leaveTime: string;
  firstMet: boolean;
  daysSinceLast: number | null;
  hasMemo: boolean;
}

export interface TimelineVisit {
  visitId: number;
  worldName: string;
  worldId: string | null;
  worldRefId: number | null;
  worldFavorite: boolean;
  worldMemo: string;
  instanceId: string;
  location: string;
  joinTime: string;
  leaveTime: string;
  duration: string;
  /** The whole visit, not just the part inside this day. */
  seconds: number;
  /** Joined before this day began and still there when it did. */
  carriedOver: boolean;
  players: TimelinePlayer[];
  photos: PhotoRow[];
}

export interface TimelineDay {
  day: string;
  start: string;
  end: string;
  totalSeconds: number;
  visitCount: number;
  peopleCount: number;
  photoCount: number;
  visits: TimelineVisit[];
  loosePhotos: PhotoRow[];
}

export interface DayActivity {
  day: string;
  seconds: number;
  visitCount: number;
  peopleCount: number;
  photoCount: number;
}

export interface MemoryYear {
  year: number;
  day: string;
  visitCount: number;
  peopleCount: number;
  photoCount: number;
  worlds: string[];
  photos: PhotoRow[];
}

export interface TopPlayer {
  playerId: number;
  userId: string;
  displayName: string;
  seconds: number;
  count: number;
}

export interface TopWorld {
  worldRefId: number | null;
  name: string;
  seconds: number;
  count: number;
}

export interface StatsSummary {
  totalSeconds: number;
  visitCount: number;
  worldCount: number;
  peopleCount: number;
  photoCount: number;
  activeDays: number;
  topPlayers: TopPlayer[];
  topWorlds: TopWorld[];
}

export interface AlbumRow {
  id: number;
  name: string;
  description: string;
  coverPhotoId: number | null;
  photoCount: number;
  createdAt: string;
  firstTaken: string | null;
  lastTaken: string | null;
}

// ---- Tags / players ----

export const listTags = () => invoke<TagCount[]>("list_tags");

export const createTag = (name: string, color: string) => invoke<Tag>("create_tag", { name, color });

export const updateTag = (id: number, name: string, color: string) =>
  invoke<void>("update_tag", { id, name, color });

export const deleteTag = (id: number) => invoke<void>("delete_tag", { id });

export const setPlayerTags = (playerId: number, tagIds: number[]) =>
  invoke<void>("set_player_tags", { playerId, tagIds });

export const setPlayerMemo = (playerId: number, memo: string) =>
  invoke<void>("set_player_memo", { playerId, memo });

export const playerProfile = (playerId: number) =>
  invoke<PlayerProfile | null>("player_profile", { playerId });

export const playersByTag = (tagId: number) => invoke<PlayerHit[]>("players_by_tag", { tagId });

export const playerMarks = (playerIds: number[]) => invoke<PlayerMark[]>("player_marks", { playerIds });

export const visitPlayerBadges = (visitId: number) =>
  invoke<PlayerBadge[]>("visit_player_badges", { visitId });

// ---- Worlds ----

export const worlds = (term: string | null, favoriteOnly: boolean, sort: WorldSort) =>
  invoke<WorldRow[]>("worlds", { term, favoriteOnly, sort });

export const setWorldNote = (worldRefId: number, memo: string, favorite: boolean) =>
  invoke<void>("set_world_note", { worldRefId, memo, favorite });

// ---- Timeline / stats ----

export const timelineDay = (day: string) => invoke<TimelineDay>("timeline_day", { day });

export const activity = (from: string, to: string) => invoke<DayActivity[]>("activity", { from, to });

export const adjacentActiveDay = (day: string, forward: boolean) =>
  invoke<string | null>("adjacent_active_day", { day, forward });

export const latestActiveDay = () => invoke<string | null>("latest_active_day");

export const onThisDay = (day: string) => invoke<MemoryYear[]>("on_this_day", { day });

export const stats = (from: string | null, to: string | null) =>
  invoke<StatsSummary>("stats", { from, to });

export const years = () => invoke<number[]>("years");

// ---- Albums ----

export const albums = () => invoke<AlbumRow[]>("albums");

export const createAlbum = (name: string, description: string) =>
  invoke<AlbumRow>("create_album", { name, description });

export const updateAlbum = (id: number, name: string, description: string, coverPhotoId: number | null) =>
  invoke<void>("update_album", { id, name, description, coverPhotoId });

export const deleteAlbum = (id: number) => invoke<void>("delete_album", { id });

export const albumAddPhotos = (albumId: number, photoIds: number[]) =>
  invoke<number>("album_add_photos", { albumId, photoIds });

export const albumRemovePhotos = (albumId: number, photoIds: number[]) =>
  invoke<void>("album_remove_photos", { albumId, photoIds });

export const albumReorder = (albumId: number, photoIds: number[]) =>
  invoke<void>("album_reorder", { albumId, photoIds });

export const albumPhotos = (albumId: number) => invoke<PhotoRow[]>("album_photos", { albumId });

export const albumsOfPhoto = (photoId: number) => invoke<number[]>("albums_of_photo", { photoId });

export const createAlbumFromVisit = (visitId: number, name: string) =>
  invoke<AlbumRow>("create_album_from_visit", { visitId, name });

// ---- Story export ----

export const exportAlbumHtml = (albumId: number, includePlayers: boolean) =>
  invoke<void>("export_album_html", { albumId, includePlayers });

export const exportDayHtml = (day: string, includePlayers: boolean) =>
  invoke<void>("export_day_html", { day, includePlayers });

// ---- Links ----

export const openWorldPage = (worldId: string) => invoke<void>("open_world_page", { worldId });

export const openUserPage = (userId: string) => invoke<void>("open_user_page", { userId });

export const openInstance = (location: string) => invoke<void>("open_instance", { location });
