import { describe, expect, it } from "vitest";
import type { PhotoRow } from "../../api";
import { buildLayout, GAP } from "./layout";

function photo(id: number, takenAt: string, width = 1600, height = 900): PhotoRow {
  return {
    id,
    path: `C:/pics/${id}.png`,
    fileName: `${id}.png`,
    takenAt,
    timeSource: "name",
    width,
    height,
    favorite: false,
    missing: false,
    onlineOnly: false,
    visitId: null,
    worldName: null,
    worldRefId: null,
  };
}

const items = (n: number, takenAt = "2026-11-19 12:00:00") => Array.from({ length: n }, (_, i) => photo(i + 1, takenAt, 1000 + i * 37, 700 + (i % 3) * 100));

describe("buildLayout", () => {
  it("is empty without a width or items", () => {
    expect(buildLayout(items(3), 0, 170, 0, false).total).toBe(0);
    expect(buildLayout([], 800, 170, 0, false).rows).toEqual([]);
  });

  it("fits every full row inside the container width", () => {
    const width = 1000;
    const layout = buildLayout(items(40), width, 170, 0, false);
    for (const row of layout.rows) {
      if (row.kind !== "row") continue;
      const sum = row.tiles.reduce((s, t) => s + t.w, 0) + GAP * (row.tiles.length - 1);
      expect(sum).toBeLessThanOrEqual(width);
    }
  });

  it("totals the heights of all rows and maps every item to a row", () => {
    const layout = buildLayout(items(25), 900, 170, 0, false);
    expect(layout.total).toBe(layout.rows.reduce((s, r) => s + r.height, 0));
    const last = layout.rows[layout.rows.length - 1];
    expect(last.top + last.height).toBe(layout.total);
    for (let i = 0; i < 25; i++) {
      const row = layout.rows[layout.rowOfItem[i]];
      expect(row.kind === "row" && row.tiles.some((t) => t.index === i)).toBe(true);
    }
  });

  it("makes a header row per logical day when grouping by day", () => {
    const list = [
      ...items(3, "2026-11-19 12:00:00"),
      ...items(2, "2026-11-18 12:00:00").map((p) => ({ ...p, id: p.id + 10 })),
    ];
    const layout = buildLayout(list, 800, 170, 0, true);
    const headers = layout.rows.filter((r) => r.kind === "header");
    expect(headers.map((h) => h.kind === "header" && h.label)).toEqual(["2026年11月19日 (木)", "2026年11月18日 (水)"]);
    expect(headers.map((h) => h.kind === "header" && h.count)).toEqual([3, 2]);
    expect(layout.rows[0].kind).toBe("header");
  });

  it("makes no header rows when not grouping", () => {
    expect(buildLayout(items(5), 800, 170, 0, false).rows.every((r) => r.kind === "row")).toBe(true);
  });
});
