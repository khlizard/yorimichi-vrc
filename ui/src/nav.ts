import { createContext, useContext, useEffect, useRef } from "react";

/** Filter for the photos tab. `label` is the bare name (world / player); the chip adds the prefix. */
export interface PhotoFilter {
  worldRefId?: number;
  playerId?: number;
  label: string;
}

/** Cross-tab navigation, provided by App. Tabs call these instead of receiving one prop per target. */
export interface AppNav {
  showPhotos(filter: PhotoFilter): void;
  showVisit(visitId: number | null): void;
  showPlayer(playerId: number, displayName: string): void;
  /** Open the timeline on a logical day (`YYYY-MM-DD`). */
  showDay(day: string): void;
  /** Open the timeline on the logical day containing this local timestamp (`YYYY-MM-DD HH:MM:SS`). */
  showDayAt(timestamp: string): void;
  /** Open the worlds tab with that world selected (`worlds.id`). */
  showWorld(worldRefId: number): void;
  /** Open the albums tab with that album selected. */
  showAlbum(albumId: number): void;
}

const noop = () => {};

export const AppNavContext = createContext<AppNav>({
  showPhotos: noop,
  showVisit: noop,
  showPlayer: noop,
  showDay: noop,
  showDayAt: noop,
  showWorld: noop,
  showAlbum: noop,
});

export function useNav(): AppNav {
  return useContext(AppNavContext);
}

// ---- Back interception (a lightbox closes on Back instead of the app navigating) ----

type BackHandler = () => boolean;
const backHandlers: BackHandler[] = [];

/**
 * While mounted (and `enabled`), the app's Back (Alt+←, mouse back button, ← button) first calls
 * `fn`. Return true when it handled Back (e.g. closed itself); false to let the next handler or the
 * history go. The newest registration is asked first.
 */
export function useBackIntercept(fn: BackHandler, enabled = true): void {
  const ref = useRef(fn);
  ref.current = fn;
  useEffect(() => {
    if (!enabled) return;
    const h: BackHandler = () => ref.current();
    backHandlers.push(h);
    return () => {
      const i = backHandlers.lastIndexOf(h);
      if (i >= 0) backHandlers.splice(i, 1);
    };
  }, [enabled]);
}

/** Asks the registered handlers, newest first. True if one of them handled Back. */
export function runBackIntercepts(): boolean {
  for (let i = backHandlers.length - 1; i >= 0; i--) {
    if (backHandlers[i]()) return true;
  }
  return false;
}

/** True if the element is visible: rendered, and not inside an inactive tab (`visibility: hidden`). */
export function isShown(el: Element | null): boolean {
  return !!el && el.checkVisibility({ visibilityProperty: true });
}
