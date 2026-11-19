import { useEffect, useState, type ReactNode } from "react";
import { onPhotosChanged, photos as fetchPhotos, photosForVisit, type PhotoRow } from "../api";
import { useLatest } from "../components/hooks";
import { photoLabel } from "./photoUtil";
import { Thumb } from "./Thumb";
import "./photos.css";

/** Photos of one visit (null = none selected). Reloads when the photo library changes. */
export function useVisitPhotos(visitId: number | null): [PhotoRow[], (f: (p: PhotoRow[]) => PhotoRow[]) => void] {
  const [list, setList] = useState<PhotoRow[]>([]);
  const idRef = useLatest(visitId);

  useEffect(() => {
    let alive = true;
    const load = () => {
      const id = idRef.current;
      if (id == null) {
        setList([]);
        return;
      }
      photosForVisit(id).then(
        (r) => alive && idRef.current === id && setList(r),
        () => alive && idRef.current === id && setList([]),
      );
    };
    load();
    const un = onPhotosChanged(load);
    return () => {
      alive = false;
      un.then((f) => f()).catch(() => {});
    };
  }, [visitId, idRef]);

  return [list, setList];
}

/** Newest photos of a world or a player (null = none selected). Stale responses are dropped; reloads on library changes. */
export function useRecentPhotos(
  filter: { worldRefId?: number; playerId?: number } | null,
  limit = 30,
): [PhotoRow[], (f: (p: PhotoRow[]) => PhotoRow[]) => void] {
  const [list, setList] = useState<PhotoRow[]>([]);
  const world = filter?.worldRefId ?? null;
  const player = filter?.playerId ?? null;
  const key = filter == null ? "" : `${world}|${player}`;
  const keyRef = useLatest(key);

  useEffect(() => {
    let alive = true;
    setList([]);
    const load = () => {
      if (keyRef.current === "") return;
      const k = keyRef.current;
      fetchPhotos({ worldRefId: world, playerId: player, limit }).then(
        (r) => alive && keyRef.current === k && setList(r),
        () => alive && keyRef.current === k && setList([]),
      );
    };
    load();
    const un = onPhotosChanged(load);
    return () => {
      alive = false;
      un.then((f) => f()).catch(() => {});
    };
  }, [key, world, player, limit, keyRef]);

  return [list, setList];
}

/** Horizontal thumbnail strip (72px high); renders nothing when empty. */
export function PhotoStrip({
  photos,
  onOpen,
  actions,
  clip,
  title,
}: {
  photos: PhotoRow[];
  onOpen: (index: number) => void;
  /** Extra controls shown on the header line. */
  actions?: ReactNode;
  /** One row, overflow cut off (no scrollbar). */
  clip?: boolean;
  /** Replaces the default "このときの写真 (n)" heading. */
  title?: string;
}) {
  if (photos.length === 0) return null;
  return (
    <div className={clip ? "strip clip" : "strip"}>
      <div className="strip-head">
        <span className="strip-title">{title ?? `このときの写真 (${photos.length.toLocaleString("ja-JP")})`}</span>
        {actions}
      </div>
      <div className="strip-row" role="group" aria-label={title ?? "このときの写真"}>
        {photos.map((p, i) => (
          <button
            key={p.id}
            type="button"
            className="strip-item"
            data-photo-id={p.id}
            aria-label={photoLabel(p)}
            style={{ aspectRatio: p.width && p.height ? `${p.width} / ${p.height}` : "16 / 9" }}
            onClick={() => onOpen(i)}
          >
            <Thumb photo={p} />
          </button>
        ))}
      </div>
    </div>
  );
}
