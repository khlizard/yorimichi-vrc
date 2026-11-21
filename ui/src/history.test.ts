import { describe, expect, it } from "vitest";
import { emptyHistory, HISTORY_MAX, recordMove, sameEntry, stepHistory, type HistEntry } from "./history";

const e = (tab: string, view?: unknown): HistEntry => ({ tab, view });

describe("sameEntry", () => {
  it("compares the tab and the JSON of the view", () => {
    expect(sameEntry(e("photos", { a: 1 }), e("photos", { a: 1 }))).toBe(true);
    expect(sameEntry(e("photos", { a: 1 }), e("photos", { a: 2 }))).toBe(false);
    expect(sameEntry(e("photos"), e("albums"))).toBe(false);
  });
  it("treats an undefined view as equal to another undefined view", () => {
    expect(sameEntry(e("timeline"), e("timeline"))).toBe(true);
    expect(sameEntry(e("timeline"), e("timeline", null))).toBe(false);
  });
});

describe("recordMove", () => {
  it("pushes the entry we leave", () => {
    const s = recordMove(emptyHistory<HistEntry>(), e("timeline"), sameEntry);
    expect(s.back).toEqual([e("timeline")]);
    expect(s.forward).toEqual([]);
  });

  it("does not stack the same screen twice in a row", () => {
    let s = recordMove(emptyHistory<HistEntry>(), e("photos", 1), sameEntry);
    s = recordMove(s, e("photos", 1), sameEntry);
    expect(s.back).toHaveLength(1);
    s = recordMove(s, e("photos", 2), sameEntry);
    expect(s.back).toHaveLength(2);
  });

  it("keeps at most HISTORY_MAX entries, dropping the oldest", () => {
    let s = emptyHistory<HistEntry>();
    for (let i = 0; i < HISTORY_MAX + 5; i++) s = recordMove(s, e("photos", i), sameEntry);
    expect(HISTORY_MAX).toBe(50);
    expect(s.back).toHaveLength(50);
    expect(s.back[0].view).toBe(5);
    expect(s.back[49].view).toBe(54);
  });

  it("clears the forward side, even when nothing is pushed", () => {
    const s0 = { back: [e("photos", 1)], forward: [e("albums")] };
    expect(recordMove(s0, e("photos", 1), sameEntry).forward).toEqual([]);
    expect(recordMove(s0, e("photos", 2), sameEntry).forward).toEqual([]);
  });

  it("does not mutate its input", () => {
    const s0 = { back: [e("a")], forward: [e("b")] };
    recordMove(s0, e("c"), sameEntry);
    expect(s0).toEqual({ back: [e("a")], forward: [e("b")] });
  });
});

describe("stepHistory", () => {
  it("goes back: pops the back stack and pushes the current screen forward", () => {
    const s0 = { back: [e("timeline"), e("photos")], forward: [] as HistEntry[] };
    const r = stepHistory(s0, -1, () => e("albums"));
    expect(r?.entry).toEqual(e("photos"));
    expect(r?.stacks).toEqual({ back: [e("timeline")], forward: [e("albums")] });
  });

  it("goes forward: pops the forward stack and pushes the current screen back", () => {
    const s0 = { back: [e("timeline")], forward: [e("albums"), e("worlds")] };
    const r = stepHistory(s0, 1, () => e("photos"));
    expect(r?.entry).toEqual(e("worlds"));
    expect(r?.stacks).toEqual({ back: [e("timeline"), e("photos")], forward: [e("albums")] });
  });

  it("returns null without touching anything when the stack is empty", () => {
    let called = false;
    const r = stepHistory(emptyHistory<HistEntry>(), -1, () => {
      called = true;
      return e("x");
    });
    expect(r).toBeNull();
    expect(called).toBe(false);
    expect(stepHistory({ back: [e("a")], forward: [] }, 1, () => e("x"))).toBeNull();
  });

  it("a new move after going back discards the forward side", () => {
    let s = recordMove(recordMove(emptyHistory<HistEntry>(), e("timeline"), sameEntry), e("photos"), sameEntry);
    s = stepHistory(s, -1, () => e("albums"))!.stacks;
    expect(s.forward).toHaveLength(1);
    s = recordMove(s, e("players"), sameEntry);
    expect(s.forward).toEqual([]);
  });
});
