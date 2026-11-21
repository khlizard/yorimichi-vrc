// Contract for writing log-derived metadata into photos (src-tauri metadata_commands.rs).
// Commands reject with a Japanese string on failure.
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type MetaMode = "write" | "remove";

export type MetaTarget = { kind: "ids"; ids: number[] } | { kind: "all" };

export type SkipReason =
  | "hasMetadata"
  | "noLog"
  | "estimatedTime"
  | "onlineOnly"
  | "missing"
  | "unsupportedFormat"
  | "readOnly"
  | "unreadable";

export const SKIP_LABELS: Record<SkipReason, string> = {
  hasMetadata: "すでにメタデータがある",
  noLog: "その時刻のログがない",
  estimatedTime: "撮影時刻が推定（ファイル名に日時がない）",
  onlineOnly: "オンライン専用（ダウンロードされていない）",
  missing: "ファイルが見つからない",
  unsupportedFormat: "対象外の形式（PNG・JPEG 以外）",
  readOnly: "書き込み禁止",
  unreadable: "読み取れない",
};

export type XmpState = "none" | "ours" | "other" | "unknown";

export interface MetaStatus {
  xmp: XmpState;
  writable: boolean;
  writableAfterDownload: boolean;
  reason: SkipReason | null;
  removable: boolean;
}

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
  /** Write plans: online-only photos in the target. */
  online: number;
  /** Write plans: online-only photos that may be writable once downloaded (not in `eligible`). */
  download: number;
  downloadIds: number[];
  downloadBytes: number;
}

export interface MetaRunSummary {
  done: number;
  skipped: number;
  failed: number;
  cancelled: boolean;
  failures: { fileName: string; message: string }[];
  /** Online-only files downloaded to be checked; they stay on this PC. */
  downloaded: number;
  downloadedBytes: number;
  lowDisk: boolean;
  downloadStopped: boolean;
}

export interface MetaProgress {
  done: number;
  total: number;
  fileName: string;
  downloading: boolean;
}

export const metadataPlan = (target: MetaTarget, mode: MetaMode) =>
  invoke<MetaPlan>("metadata_plan", { target, mode });
export const metadataRun = (target: MetaTarget, mode: MetaMode, allowDownload = false) =>
  invoke<MetaRunSummary>("metadata_run", { target, mode, allowDownload });
export const photoMetaStatus = (id: number) => invoke<MetaStatus>("photo_meta_status", { id });
export const metadataCancel = () => invoke<void>("metadata_cancel");

export const onMetadataProgress = (cb: (p: MetaProgress) => void): Promise<UnlistenFn> =>
  listen<MetaProgress>("metadata-progress", (e) => cb(e.payload));
/** Fired by the File menu: write metadata to every photo that can be written. */
export const onOpenMetadataDialog = (cb: () => void): Promise<UnlistenFn> =>
  listen<null>("open-metadata-dialog", () => cb());
