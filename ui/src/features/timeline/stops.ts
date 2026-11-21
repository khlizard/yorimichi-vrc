import type { PhotoRow } from "../../api";
import type { TimelineDay, TimelineVisit } from "../../libraryApi";

export type Stop =
  | { kind: "visit"; key: string; time: string; visit: TimelineVisit }
  | { kind: "loose"; key: string; time: string; photos: PhotoRow[] };

export function buildStops(data: TimelineDay): Stop[] {
  const visits = [...data.visits].sort((a, b) => a.joinTime.localeCompare(b.joinTime));
  const stops: Stop[] = visits.map((v) => ({ kind: "visit", key: `v${v.visitId}`, time: v.joinTime, visit: v }));
  // Loose photos go into the slot between the visits they fall between.
  const slots = new Map<number, PhotoRow[]>();
  const loose = [...data.loosePhotos].sort((a, b) => a.takenAt.localeCompare(b.takenAt));
  for (const p of loose) {
    let slot = 0;
    while (slot < visits.length && visits[slot].joinTime <= p.takenAt) slot++;
    const l = slots.get(slot);
    if (l) l.push(p);
    else slots.set(slot, [p]);
  }
  for (const [slot, photos] of slots) stops.push({ kind: "loose", key: `l${slot}`, time: photos[0].takenAt, photos });
  stops.sort((a, b) => a.time.localeCompare(b.time) || (a.kind === "visit" ? -1 : 1));
  return stops;
}
