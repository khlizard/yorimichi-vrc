import { useState } from "react";
import type { PhotoRow } from "../../api";

/**
 * Toggle `id` in a selection. With a non-empty `range` (Shift+click: ids from the last toggled photo to
 * this one) all of them are added instead.
 */
export function toggleSelection(s: Set<number>, id: number, range?: number[]): Set<number> {
  const n = new Set(s);
  if (range && range.length > 0) range.forEach((x) => n.add(x));
  else if (n.has(id)) n.delete(id);
  else n.add(id);
  return n;
}

/** The list with one photo's favorite flag replaced (the other photos are kept as they are). */
export function withFavorite(list: PhotoRow[], id: number, favorite: boolean): PhotoRow[] {
  return list.map((x) => (x.id === id ? { ...x, favorite } : x));
}

/** "Select" mode of a photo grid and the selected photo ids. */
export function usePhotoSelection() {
  const [selecting, setSelecting] = useState(false);
  const [sel, setSel] = useState<Set<number>>(() => new Set());
  return {
    selecting,
    sel,
    setSel,
    /** For PhotoGrid's onToggleSelect. */
    toggle: (id: number, range?: number[]) => setSel((s) => toggleSelection(s, id, range)),
    /** Enter / leave select mode; the selection starts over either way. */
    toggleSelecting: () => {
      setSelecting((s) => !s);
      setSel(new Set());
    },
  };
}
