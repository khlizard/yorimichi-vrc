import { createContext, useContext } from "react";
import type { PhotoFilter } from "./photos/nav";

/** Cross-tab navigation, provided by App. Tabs call these instead of receiving one prop per target. */
export interface AppNav {
  showPhotos(filter: PhotoFilter): void;
  showVisit(visitId: number | null): void;
  showPlayer(playerId: number, displayName: string): void;
  /** Open the timeline on a logical day (`YYYY-MM-DD`). */
  showDay(day: string): void;
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
  showWorld: noop,
  showAlbum: noop,
});

export function useNav(): AppNav {
  return useContext(AppNavContext);
}
