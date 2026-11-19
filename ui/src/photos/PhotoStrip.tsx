import { useEffect, useState, type ReactNode } from "react";
import { onPhotosChanged, photosForVisit, type PhotoRow } from "../api";
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

/** Horizontal thumbnail strip (72px high); renders nothing when empty. */
export function PhotoStrip({
  photos,
  onOpen,
  actions,
}: {
  photos: PhotoRow[];
  onOpen: (index: number) => void;
  /** Extra controls shown on the header line. */
  actions?: ReactNode;
}) {
  if (photos.length === 0) return null;
  return (
    <div className="strip">
      <div className="strip-head">
        <span className="strip-title">このときの写真 ({photos.length.toLocaleString("ja-JP")})</span>
        {actions}
      </div>
      <div className="strip-row" role="group" aria-label="このときの写真">
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
