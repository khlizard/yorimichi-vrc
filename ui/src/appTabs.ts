import { PLAYER_PAGE_SIZE, RAW_PAGE_SIZE } from "./api";
import type { TabHandle } from "./components/util";
import type { AlbumsHandle } from "./features/albums/AlbumsTab";
import type { PhotosHandle } from "./features/photos/PhotosTab";
import type { PlayersHandle } from "./features/players/PlayersTab";
import type { SessionsHandle } from "./features/sessions/SessionsTab";
import type { TimelineHandle } from "./features/timeline/TimelineTab";
import type { WorldsHandle } from "./features/worlds/WorldsTab";

interface TabDef {
  id: string;
  label: string;
  autoRefresh: "always" | "ifFirstPage" | "never";
  /** Rows of one page, for "ifFirstPage". */
  pageSize?: number;
  autoOrder?: number;
}

/**
 * Every tab, in display order (also the order of "refresh all").
 * `autoRefresh` is what an automatic import does to the tab:
 * - "always": reload it,
 * - "ifFirstPage": reload it only while at most `pageSize` rows are loaded (a long paged list the
 *   user scrolled into is left alone and the status line says so),
 * - "never": leave it.
 * `autoOrder` is the order those reloads are started in.
 */
export const TABS = [
  { id: "timeline", label: "タイムライン", autoRefresh: "always", autoOrder: 1 },
  { id: "photos", label: "写真", autoRefresh: "never" },
  { id: "albums", label: "アルバム", autoRefresh: "always", autoOrder: 2 },
  { id: "players", label: "プレイヤー", autoRefresh: "ifFirstPage", pageSize: PLAYER_PAGE_SIZE, autoOrder: 9 },
  { id: "sessions", label: "セッション", autoRefresh: "always", autoOrder: 5 },
  { id: "worlds", label: "ワールド", autoRefresh: "always", autoOrder: 3 },
  { id: "urls", label: "URL", autoRefresh: "always", autoOrder: 6 },
  { id: "avatars", label: "アバター", autoRefresh: "always", autoOrder: 7 },
  { id: "notifications", label: "通知", autoRefresh: "always", autoOrder: 8 },
  { id: "logs", label: "生ログ", autoRefresh: "ifFirstPage", pageSize: RAW_PAGE_SIZE, autoOrder: 10 },
  { id: "stats", label: "統計", autoRefresh: "always", autoOrder: 4 },
] as const satisfies readonly TabDef[];
export type TabId = (typeof TABS)[number]["id"];

/** Imperative handle of every tab. Refresh is explicit: App calls handle.refresh(). */
export interface TabHandles {
  timeline: TimelineHandle;
  photos: PhotosHandle;
  albums: AlbumsHandle;
  players: PlayersHandle;
  sessions: SessionsHandle;
  worlds: WorldsHandle;
  urls: TabHandle;
  avatars: TabHandle;
  notifications: TabHandle;
  logs: TabHandle;
  stats: TabHandle;
}
export type TabRefs = { [K in TabId]: { current: TabHandles[K] | null } };

/** What an automatic import reloads, in the order it starts them. */
const defs: readonly (TabDef & { id: TabId })[] = TABS;
export const AUTO_REFRESH_TABS = defs.filter((t) => t.autoRefresh !== "never").sort((a, b) => (a.autoOrder ?? 0) - (b.autoOrder ?? 0));
