// Demo mode: replaces the Tauri IPC with dummy data so the UI runs in a plain browser.
// Loaded only by the dev server with `?demo` (see main.tsx); never part of a release build.
// `?tab=<id>` opens that tab, `?theme=dark` switches the theme, `?open=period` opens the period dialog.
import { emit } from "@tauri-apps/api/event";
import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import type { Settings } from "../api";
import * as data from "./data";

const params = new URLSearchParams(location.search);
const theme = params.get("theme") === "dark" ? "dark" : "light";
const settings: Settings = { ...data.settings, Theme: theme };
try {
  localStorage.setItem("yorimichi.theme", theme);
} catch {
  /* ignore */
}

type Args = Record<string, unknown>;
const handlers: Record<string, (a: Args) => unknown> = {
  get_status: () => data.status,
  get_settings: () => settings,
  autostart_get: () => false,
  default_noise_patterns: () => [],
  latest_active_day: () => data.DEMO_DAY,
  timeline_day: () => data.timelineDay,
  on_this_day: () => data.memories,
  adjacent_active_day: (a) => (a.forward ? null : "2026-11-19"),
  visit_player_badges: (a) => data.visitBadges(Number(a.visitId)),
  player_marks: (a) => data.playerMarks((a.playerIds as number[]) ?? []),
  recent_players: () => data.recentPlayers,
  years: () => data.years,
  stats: (a) => data.statsFor((a.from as string | null) ?? null, (a.to as string | null) ?? null),
  activity: (a) => data.activityFor(String(a.from), String(a.to)),
  notification_kinds: () => [],
  photo_detail: () => null,
  player_profile: () => null,
};

mockWindows("main");
mockIPC(
  (cmd, payload) => {
    const h = handlers[cmd];
    if (h) return h((payload ?? {}) as Args);
    if (cmd.startsWith("plugin:")) return null;
    // Lists default to empty; anything else that returns a value gets null.
    return [];
  },
  { shouldMockEvents: true },
);

// Photo thumbnails: a soft gradient per photo id instead of the vrcphoto:// scheme.
const SKIES = [
  ["#8ec5fc", "#e0c3fc"],
  ["#f6d365", "#fda085"],
  ["#84fab0", "#8fd3f4"],
  ["#a1c4fd", "#c2e9fb"],
  ["#fbc2eb", "#a6c1ee"],
  ["#ffecd2", "#fcb69f"],
  ["#30cfd0", "#330867"],
  ["#5ee7df", "#b490ca"],
];
function picture(path: string): string {
  const id = Number(path.split("/").pop()) || 0;
  const [a, b] = SKIES[id % SKIES.length];
  const hill = id % 2 === 0 ? "#3d5a80" : "#46634a";
  const svg =
    `<svg xmlns="http://www.w3.org/2000/svg" width="320" height="180" viewBox="0 0 320 180">` +
    `<defs><linearGradient id="g" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="${a}"/><stop offset="1" stop-color="${b}"/></linearGradient></defs>` +
    `<rect width="320" height="180" fill="url(#g)"/>` +
    `<circle cx="${60 + ((id * 47) % 200)}" cy="${40 + ((id * 13) % 30)}" r="14" fill="#fff" opacity="0.7"/>` +
    `<path d="M0 140 Q80 ${100 + (id % 3) * 10} 160 135 T320 ${125 + (id % 4) * 5} V180 H0Z" fill="${hill}" opacity="0.85"/>` +
    `</svg>`;
  return `data:image/svg+xml;charset=utf-8,${encodeURIComponent(svg)}`;
}
(window as unknown as { __TAURI_INTERNALS__: { convertFileSrc: (p: string, proto?: string) => string } }).__TAURI_INTERNALS__.convertFileSrc =
  (p) => picture(p);

/** Runs `f` once the app has rendered its tabs (and so has subscribed to backend events). */
function whenReady(f: () => void) {
  if (document.getElementById("tab-timeline")) setTimeout(f, 100);
  else setTimeout(() => whenReady(f), 50);
}

const tab = params.get("tab");
if (tab) whenReady(() => document.getElementById(`tab-${tab}`)?.click());

// `?open=period`: what the menu item 表示 → 期間で絞り込む… sends.
if (params.get("open") === "period") whenReady(() => void emit("open-period-dialog"));
