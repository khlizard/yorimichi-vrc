// Dummy data for the demo mode (dev server, `?demo`). Used for screenshots in README / docs.
// Every world, player and ID here is made up.
import type { PhotoRow, PlayerHit, Settings, Status } from "../api";
import type { DayActivity, MemoryYear, PlayerBadge, PlayerMark, StatsSummary, TimelineDay, TimelinePlayer, TimelineVisit, TopPlayer, TopWorld } from "../libraryApi";
import { pad } from "../format";

export const DEMO_DAY = "2026-11-21";

const uuid = (n: number) => `00000000-0000-4000-8000-${String(n).padStart(12, "0")}`;

const WORLDS = [
  { ref: 1, name: "星見の丘テラス" },
  { ref: 2, name: "ランタン横丁の喫茶店" },
  { ref: 3, name: "Aurora Pier" },
  { ref: 4, name: "雨宿りの図書館" },
  { ref: 5, name: "Cozy Attic Room" },
  { ref: 6, name: "Neon Arcade 2088" },
  { ref: 7, name: "海辺の小さな駅" },
  { ref: 8, name: "Sakura Rooftop" },
] as const;

const PLAYERS = [
  "Hoshino", "mochi_mochi", "Rin", "かえで", "Nagi", "ことり", "Sora_7", "つむぎ", "Yuzu", "Haru", "Mika", "しろくま", "Kuro", "Ao",
].map((name, i) => ({ id: i + 1, userId: `usr_${uuid(i + 1)}`, name }));

const worldId = (ref: number) => `wrld_${uuid(100 + ref)}`;
const sec = (t: string) => {
  const [h, m] = t.split(":").map(Number);
  return h * 3600 + m * 60;
};
const at = (day: string, t: string) => `${day} ${t}:00`;

let photoSeq = 1;
function photo(day: string, t: string, visitId: number | null, world: string | null, ref: number | null, wide = true): PhotoRow {
  const id = photoSeq++;
  return {
    id,
    path: `C:\\Pictures\\VRChat\\VRChat_${day}_${t.replace(":", "-")}-00.000_${wide ? "1920x1080" : "1080x1920"}.png`,
    fileName: `VRChat_${day}_${t.replace(":", "-")}-00.000.png`,
    takenAt: at(day, t),
    timeSource: "filename",
    width: wide ? 1920 : 1080,
    height: wide ? 1080 : 1920,
    favorite: id % 5 === 0,
    missing: false,
    onlineOnly: false,
    visitId,
    worldName: world,
    worldRefId: ref,
  };
}

interface VisitSpec {
  world: number;
  join: string;
  leave: string;
  /** [player index, join, leave, firstMet, daysSinceLast] */
  people: [number, string, string, boolean?, number?][];
  photos: string[];
  favorite?: boolean;
  memo?: string;
}

const VISITS: VisitSpec[] = [
  {
    world: 5, join: "20:12", leave: "20:41",
    people: [[0, "20:15", "20:41"], [1, "20:18", "20:41"]],
    photos: [],
  },
  {
    world: 1, join: "20:43", leave: "21:36", favorite: true,
    people: [[0, "20:43", "21:36"], [1, "20:43", "21:36"], [2, "20:50", "21:36", false, 12], [3, "20:52", "21:20"], [4, "21:01", "21:36", true], [5, "21:05", "21:36"]],
    photos: ["20:58", "21:07", "21:12", "21:24"],
  },
  {
    world: 2, join: "21:38", leave: "22:47", memo: "ケーキがおいしそう",
    people: [[0, "21:38", "22:47"], [2, "21:38", "22:47"], [5, "21:40", "22:30"], [6, "21:44", "22:47", true], [7, "21:52", "22:47", false, 34], [8, "22:01", "22:47"], [9, "22:05", "22:40"]],
    photos: ["21:46", "21:58", "22:11", "22:20", "22:33"],
  },
  {
    world: 3, join: "22:49", leave: "23:55",
    people: [[0, "22:49", "23:55"], [2, "22:49", "23:55"], [7, "22:49", "23:30"], [10, "23:02", "23:55"], [11, "23:10", "23:55", true]],
    photos: ["23:05", "23:18", "23:41"],
  },
  {
    world: 4, join: "23:57", leave: "24:48",
    people: [[0, "23:57", "24:48"], [12, "24:05", "24:48"]],
    photos: ["24:20"],
  },
];

/** "24:48" on DEMO_DAY -> next calendar day 00:48 (the day starts at 6:00). */
function stamp(t: string): string {
  const [h, m] = t.split(":").map(Number);
  if (h < 24) return at(DEMO_DAY, t);
  return at("2026-11-22", `${pad(h - 24)}:${pad(m)}`);
}

function dur(join: string, leave: string): { seconds: number; text: string } {
  const s = sec(leave) - sec(join);
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  return { seconds: s, text: h >= 1 ? `${h}h ${pad(m)}m` : `${m}m 00s` };
}

const visits: TimelineVisit[] = VISITS.map((v, i) => {
  const w = WORLDS.find((x) => x.ref === v.world)!;
  const visitId = 100 + i;
  const instance = `${10000 + i * 731}~group(grp_${uuid(900)})~region(jp)`;
  const d = dur(v.join, v.leave);
  const players: TimelinePlayer[] = v.people.map(([pi, j, l, first, since]) => ({
    playerId: PLAYERS[pi].id,
    userId: PLAYERS[pi].userId,
    displayName: PLAYERS[pi].name,
    joinTime: stamp(j),
    leaveTime: stamp(l),
    firstMet: first ?? false,
    daysSinceLast: since ?? null,
    hasMemo: pi === 2,
  }));
  return {
    visitId,
    worldName: w.name,
    worldId: worldId(w.ref),
    worldRefId: w.ref,
    worldFavorite: v.favorite ?? false,
    worldMemo: v.memo ?? "",
    instanceId: instance,
    location: `${worldId(w.ref)}:${instance}`,
    joinTime: stamp(v.join),
    leaveTime: stamp(v.leave),
    duration: d.text,
    seconds: d.seconds,
    players,
    photos: v.photos.map((t, k) => {
      const s = stamp(t);
      return photo(s.slice(0, 10), s.slice(11, 16), visitId, w.name, w.ref, k % 4 !== 3);
    }),
  };
});

const people = new Set(visits.flatMap((v) => v.players.map((p) => p.playerId)));

export const timelineDay: TimelineDay = {
  day: DEMO_DAY,
  start: visits[0].joinTime,
  end: visits[visits.length - 1].leaveTime,
  totalSeconds: visits.reduce((a, v) => a + v.seconds, 0),
  visitCount: visits.length,
  peopleCount: people.size - 1,
  photoCount: visits.reduce((a, v) => a + v.photos.length, 0),
  visits,
  loosePhotos: [],
};

export const memories: MemoryYear[] = [
  {
    year: 2025,
    day: "2025-11-21",
    visitCount: 3,
    peopleCount: 6,
    photoCount: 4,
    worlds: [WORLDS[6].name, WORLDS[7].name, WORLDS[4].name],
    photos: ["22:10", "22:24", "23:02", "23:15"].map((t) => photo("2025-11-21", t, 50, WORLDS[6].name, WORLDS[6].ref)),
  },
];

export const visitBadges = (visitId: number): PlayerBadge[] =>
  visits
    .find((v) => v.visitId === visitId)
    ?.players.map((p) => ({ playerId: p.playerId, firstMet: p.firstMet, daysSinceLast: p.daysSinceLast })) ?? [];

export const playerMarks = (ids: number[]): PlayerMark[] =>
  ids.map((id) => ({ playerId: id, hasMemo: id === PLAYERS[2].id, tags: id === PLAYERS[0].id ? [{ id: 1, name: "いつもの", color: "#4f8a8b" }] : [] }));

export const recentPlayers: PlayerHit[] = PLAYERS.map((p, i) => ({
  id: p.id,
  userId: p.userId,
  displayName: p.name,
  isLocal: false,
  otherNames: null,
  encounterCount: 40 - i * 2,
  lastSeen: at(DEMO_DAY, "23:00"),
  lastSeenDate: DEMO_DAY,
}));

// ---- Stats ----

/** Deterministic pseudo-random in [0, 1) from a string (same picture on every run). */
function rand(s: string): number {
  let h = 2166136261;
  for (let i = 0; i < s.length; i++) h = Math.imul(h ^ s.charCodeAt(i), 16777619);
  h ^= h >>> 13;
  h = Math.imul(h, 0x5bd1e995);
  h ^= h >>> 15;
  return (h >>> 0) / 4294967296;
}

export function activityFor(from: string, to: string): DayActivity[] {
  const out: DayActivity[] = [];
  const start = Date.parse(`${from}T00:00:00Z`);
  const end = Math.min(Date.parse(`${to}T00:00:00Z`), Date.parse(`${DEMO_DAY}T00:00:00Z`));
  for (let t = start; t <= end; t += 86400000) {
    const d = new Date(t);
    const day = `${d.getUTCFullYear()}-${pad(d.getUTCMonth() + 1)}-${pad(d.getUTCDate())}`;
    const weekend = d.getUTCDay() === 0 || d.getUTCDay() === 5 || d.getUTCDay() === 6;
    const r = rand(day);
    if (r < (weekend ? 0.2 : 0.55)) continue;
    const hours = 0.3 + rand(`${day}h`) ** 1.6 * (weekend ? 5.5 : 3.5);
    const seconds = Math.round(hours * 3600);
    out.push({
      day,
      seconds,
      visitCount: 1 + Math.floor(hours * 1.2),
      peopleCount: Math.floor(hours * 3 * rand(`${day}p`)),
      photoCount: rand(`${day}f`) < 0.5 ? 0 : Math.floor(rand(`${day}g`) * hours * 4),
    });
  }
  return out;
}

const topPlayers: TopPlayer[] = PLAYERS.slice(0, 10).map((p, i) => ({
  playerId: p.id,
  userId: p.userId,
  displayName: p.name,
  seconds: Math.round(160 * 0.84 ** i * 3600),
  count: 96 - i * 8,
}));

const topWorlds: TopWorld[] = [5, 1, 2, 3, 7, 4, 8, 6].map((ref, i) => ({
  worldRefId: ref,
  name: WORLDS.find((w) => w.ref === ref)!.name,
  seconds: Math.round((140 - i * 14) * 3600 * 0.8),
  count: 120 - i * 13,
}));

export function statsFor(from: string | null, to: string | null): StatsSummary {
  const acts = activityFor(from ?? "2025-01-01", to ?? DEMO_DAY);
  return {
    totalSeconds: acts.reduce((a, x) => a + x.seconds, 0),
    visitCount: acts.reduce((a, x) => a + x.visitCount, 0),
    worldCount: 214,
    peopleCount: 387,
    photoCount: acts.reduce((a, x) => a + x.photoCount, 0),
    activeDays: acts.length,
    topPlayers,
    topWorlds,
  };
}

export const years = [2026, 2025];

// ---- App shell ----

export const settings: Settings = {
  DbPath: "yorimichi.db",
  LogFolders: [],
  AutoImportIntervalMinutes: 1,
  MinimizeToTray: true,
  RawLogRetentionDays: 30,
  RawLogNoisePatterns: [],
  LastViewerExportFolder: null,
  TrayChoiceMade: true,
  PhotoFolders: [],
  DayStartHour: 6,
  ShowFirstMet: true,
  ReunionDays: 7,
  AutoVacuum: false,
  AutoVacuumMinDbMb: 64,
  BackupFolder: "backup",
  BackupIntervalDays: 7,
  BackupKeepCount: 5,
  LastBackupAt: null,
  UpdateFeed: "",
  Theme: "light",
};

export const status: Status = {
  status: "取り込み完了",
  importing: false,
  dbPath: "yorimichi.db",
  dbSizeBytes: 48 * 1024 * 1024,
  reclaimableBytes: 0,
  vacuumRecommended: false,
  version: "1.0.0",
};
