import { describe, expect, it } from "vitest";
import type { PhotoRow } from "../../api";
import type { TimelineDay, TimelineVisit } from "../../libraryApi";
import { buildStops } from "./stops";

function photo(id: number, takenAt: string): PhotoRow {
  return {
    id,
    path: `C:/pics/${id}.png`,
    fileName: `${id}.png`,
    takenAt,
    timeSource: "name",
    width: 1920,
    height: 1080,
    favorite: false,
    missing: false,
    onlineOnly: false,
    visitId: null,
    worldName: null,
    worldRefId: null,
  };
}

function visit(visitId: number, joinTime: string): TimelineVisit {
  return {
    visitId,
    worldName: "Sample World",
    worldId: null,
    worldRefId: null,
    worldFavorite: false,
    worldMemo: "",
    instanceId: "1",
    location: "",
    joinTime,
    leaveTime: joinTime,
    duration: "",
    seconds: 0,
    carriedOver: false,
    players: [],
    photos: [],
  };
}

function day(visits: TimelineVisit[], loosePhotos: PhotoRow[]): TimelineDay {
  return { day: "2026-11-19", start: "", end: "", totalSeconds: 0, visitCount: visits.length, peopleCount: 0, photoCount: 0, visits, loosePhotos };
}

describe("buildStops", () => {
  it("is empty for an empty day", () => {
    expect(buildStops(day([], []))).toEqual([]);
  });

  it("sorts visits by join time", () => {
    const stops = buildStops(day([visit(2, "2026-11-19 21:00:00"), visit(1, "2026-11-19 20:00:00")], []));
    expect(stops.map((s) => s.key)).toEqual(["v1", "v2"]);
  });

  it("puts loose photos into the slot between the visits they fall between", () => {
    const stops = buildStops(
      day(
        [visit(1, "2026-11-19 20:00:00"), visit(2, "2026-11-19 22:00:00")],
        [photo(3, "2026-11-19 22:30:00"), photo(1, "2026-11-19 19:00:00"), photo(2, "2026-11-19 21:00:00"), photo(4, "2026-11-19 21:30:00")],
      ),
    );
    expect(stops.map((s) => s.key)).toEqual(["l0", "v1", "l1", "v2", "l2"]);
    const l1 = stops[2];
    expect(l1.kind === "loose" && l1.photos.map((p) => p.id)).toEqual([2, 4]);
    expect(l1.time).toBe("2026-11-19 21:00:00");
  });

  it("treats a photo taken exactly at the join time as part of that visit's slot", () => {
    const stops = buildStops(day([visit(1, "2026-11-19 20:00:00")], [photo(1, "2026-11-19 20:00:00")]));
    expect(stops.map((s) => s.key)).toEqual(["v1", "l1"]);
  });

  it("puts a visit before a loose slot with the same time", () => {
    const stops = buildStops(day([visit(1, "2026-11-19 20:00:00")], []));
    expect(stops[0].kind).toBe("visit");
  });
});
