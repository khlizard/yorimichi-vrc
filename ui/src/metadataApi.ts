// Contract for writing log-derived metadata into photos (src-tauri metadata_commands.rs).
// Commands reject with a Japanese string on failure.
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type MetaMode = "write" | "remove";

export type MetaTarget =
  | { kind: "ids"; ids: number[] }
  | { kind: "day"; day: string }
  | { kind: "album"; albumId: number }
  | { kind: "range"; from: string; to: string };

export type SkipReason =
  | "hasMetadata"
  | "noLog"
  | "estimatedTime"
  | "onlineOnly"
  | "missing"
  | "unsupportedFormat"
  | "readOnly"
  | "unreadable";

export interface MetaExample {
  fileName: string;
  takenAt: string;
  worldName: string;
  worldId: string;
  author: string | null;
}

export interface MetaPlan {
  total: number;
  eligible: number;
  eligibleIds: number[];
  skipped: { reason: SkipReason; count: number }[];
  example: MetaExample | null;
}

export interface MetaRunSummary {
  done: number;
  skipped: number;
  failed: number;
  cancelled: boolean;
  failures: { fileName: string; message: string }[];
}

export interface MetaProgress {
  done: number;
  total: number;
  fileName: string;
}

export const metadataPlan = (target: MetaTarget, mode: MetaMode) =>
  invoke<MetaPlan>("metadata_plan", { target, mode });
export const metadataRun = (target: MetaTarget, mode: MetaMode) =>
  invoke<MetaRunSummary>("metadata_run", { target, mode });
export const metadataCancel = () => invoke<void>("metadata_cancel");

export const onMetadataProgress = (cb: (p: MetaProgress) => void): Promise<UnlistenFn> =>
  listen<MetaProgress>("metadata-progress", (e) => cb(e.payload));
export const onOpenMetadataDialog = (cb: () => void): Promise<UnlistenFn> =>
  listen("open-metadata-dialog", () => cb());
