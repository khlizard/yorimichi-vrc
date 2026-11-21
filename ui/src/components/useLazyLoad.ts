import { useEffect, useMemo, useRef } from "react";
import { useLatest } from "./hooks";

/**
 * Generation counter for async loads: take `next()` when a load starts and drop the result if
 * `isCurrent(g)` is false by the time it arrives (a newer load or a changed condition replaced it).
 */
export function useGeneration() {
  const n = useRef(0);
  return useMemo(
    () => ({
      next: () => ++n.current,
      current: () => n.current,
      isCurrent: (g: number) => g === n.current,
    }),
    [],
  );
}

interface LazyLoadOptions {
  /** The tab is shown. Nothing is loaded before the first time. */
  active: boolean;
  /** Runs once, the first time the tab is shown. */
  first(): void;
  /** Reloads what the tab shows now (keeping the selection). */
  reload(): void;
  /** false while there is nothing to reload yet (default: always ready). */
  ready?(): boolean;
}

/**
 * "Load when first shown, and while hidden only remember that a reload is due":
 * - the first time `active` becomes true, `first` runs;
 * - `refresh()` reloads right away while shown, or marks the tab dirty while hidden, and a dirty
 *   tab reloads when it is shown again;
 * - `begin()` is for a caller that drives the tab itself (it loads on its own): the tab counts as
 *   started and not dirty.
 * Which data a reload fetches stays with the tab.
 */
export function useLazyLoad(o: LazyLoadOptions) {
  const started = useRef(false);
  const dirty = useRef(false);
  const latest = useLatest(o);

  useEffect(() => {
    if (!o.active) return;
    if (!started.current) {
      started.current = true;
      o.first();
    } else if (dirty.current && (o.ready?.() ?? true)) {
      dirty.current = false;
      o.reload();
    }
    // Only the tab being shown triggers this; the callbacks are the latest of that render.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [o.active]);

  return {
    started,
    dirty,
    refresh() {
      if (!started.current) return;
      const cur = latest.current;
      if (!cur.active) dirty.current = true;
      else if (cur.ready?.() ?? true) cur.reload();
    },
    begin() {
      started.current = true;
      dirty.current = false;
    },
  };
}
