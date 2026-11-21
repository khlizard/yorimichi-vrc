import type { PhotoRow } from "../../api";
import { dayLabel, logicalDay } from "./photoUtil";

export const GAP = 2;
const HEADER_H = 36;
const FALLBACK_AR = 16 / 9;

export interface Tile {
  index: number;
  w: number;
  h: number;
}
export type VRow =
  | { kind: "header"; key: string; top: number; height: number; label: string; count: number }
  | { kind: "row"; key: string; top: number; height: number; tiles: Tile[] };
export interface Layout {
  rows: VRow[];
  /** Row index of every item. */
  rowOfItem: Int32Array;
  total: number;
}

function aspect(p: PhotoRow): number {
  const ar = p.width && p.height ? p.width / p.height : FALLBACK_AR;
  return Math.min(4, Math.max(0.25, ar));
}

export function buildLayout(items: PhotoRow[], width: number, target: number, dayStart: number, byDay: boolean): Layout {
  const rows: VRow[] = [];
  const rowOfItem = new Int32Array(items.length);
  let top = 0;
  if (width <= 0 || items.length === 0) return { rows, rowOfItem, total: 0 };

  const emit = (from: number, to: number, h: number) => {
    const hh = Math.max(1, Math.round(h));
    const tiles: Tile[] = [];
    for (let k = from; k < to; k++) {
      tiles.push({ index: k, w: Math.max(1, Math.floor(aspect(items[k]) * h)), h: hh });
      rowOfItem[k] = rows.length;
    }
    const height = hh + GAP;
    rows.push({ kind: "row", key: `r${items[from].id}`, top, height, tiles });
    top += height;
  };

  const run = (start: number, end: number) => {
    let rowStart = start;
    let sum = 0;
    for (let k = start; k < end; k++) {
      sum += aspect(items[k]);
      const avail = width - GAP * (k - rowStart);
      if (sum * target >= avail) {
        emit(rowStart, k + 1, avail / sum);
        rowStart = k + 1;
        sum = 0;
      }
    }
    if (rowStart < end) emit(rowStart, end, target);
  };

  if (!byDay) {
    run(0, items.length);
  } else {
    let s = 0;
    let day = logicalDay(items[0].takenAt, dayStart);
    for (let i = 1; i <= items.length; i++) {
      const d = i < items.length ? logicalDay(items[i].takenAt, dayStart) : "";
      if (d === day) continue;
      rows.push({ kind: "header", key: `h${day}`, top, height: HEADER_H, label: dayLabel(day), count: i - s });
      top += HEADER_H;
      run(s, i);
      s = i;
      day = d;
    }
  }
  return { rows, rowOfItem, total: top };
}
