import { describe, expect, it } from "vitest";
import type { PhotoRow } from "../../api";
import { toggleSelection, withFavorite } from "./usePhotoSelection";

const photo = (id: number, favorite = false): PhotoRow => ({
  id,
  path: `C:/pics/${id}.png`,
  fileName: `${id}.png`,
  takenAt: "2026-11-19 12:00:00",
  timeSource: "name",
  width: 100,
  height: 100,
  favorite,
  missing: false,
  onlineOnly: false,
  visitId: null,
  worldName: null,
  worldRefId: null,
});

describe("toggleSelection", () => {
  it("adds and removes a single id without touching the input", () => {
    const s0 = new Set([1]);
    const s1 = toggleSelection(s0, 2);
    expect([...s1]).toEqual([1, 2]);
    expect([...toggleSelection(s1, 1)]).toEqual([2]);
    expect([...s0]).toEqual([1]);
  });

  it("adds the whole range on Shift+click, never removing", () => {
    const s = toggleSelection(new Set([2]), 4, [2, 3, 4]);
    expect([...s].sort()).toEqual([2, 3, 4]);
  });

  it("treats an empty range as a plain toggle", () => {
    expect([...toggleSelection(new Set([5]), 5, [])]).toEqual([]);
  });
});

describe("withFavorite", () => {
  it("replaces only the matching photo", () => {
    const list = [photo(1), photo(2)];
    const out = withFavorite(list, 2, true);
    expect(out.map((p) => p.favorite)).toEqual([false, true]);
    expect(out[0]).toBe(list[0]);
    expect(list[1].favorite).toBe(false);
  });
});
