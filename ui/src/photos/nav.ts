import { createContext, useContext } from "react";

/** Filter for the photos tab. `label` is the bare name (world / player); the chip adds the prefix. */
export interface PhotoFilter {
  worldRefId?: number;
  playerId?: number;
  label: string;
}

export interface PhotoNav {
  /** Switch to the photos tab filtered by a world or a player. */
  showPhotos(filter: PhotoFilter): void;
}

export const PhotoNavContext = createContext<PhotoNav>({ showPhotos: () => {} });

export function usePhotoNav(): PhotoNav {
  return useContext(PhotoNavContext);
}
