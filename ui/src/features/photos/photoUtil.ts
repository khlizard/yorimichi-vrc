import type { PhotoRow } from "../../api";
import { pad } from "../../format";

const WEEK = "日月火水木金土";

/** Logical day ("yyyy-MM-dd") of a "yyyy-MM-dd HH:mm:ss" time: before startHour counts as the previous day. */
export function logicalDay(takenAt: string, startHour: number): string {
  const h = Number(takenAt.slice(11, 13));
  if (!(h < startHour)) return takenAt.slice(0, 10);
  const t = new Date(Date.UTC(Number(takenAt.slice(0, 4)), Number(takenAt.slice(5, 7)) - 1, Number(takenAt.slice(8, 10)) - 1));
  return `${t.getUTCFullYear()}-${pad(t.getUTCMonth() + 1)}-${pad(t.getUTCDate())}`;
}

/** "2026-11-19" -> "2026年11月19日 (木)" */
export function dayLabel(day: string): string {
  const y = Number(day.slice(0, 4));
  const m = Number(day.slice(5, 7));
  const d = Number(day.slice(8, 10));
  const w = WEEK[new Date(Date.UTC(y, m - 1, d)).getUTCDay()];
  return `${y}年${m}月${d}日 (${w})`;
}

/** Time + world, used as the accessible name of a photo. */
export function photoLabel(p: PhotoRow): string {
  return `${p.takenAt.slice(11, 16)} ${p.worldName ?? ""}`.trim();
}
