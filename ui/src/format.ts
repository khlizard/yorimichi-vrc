/** Integer / decimal with the ja-JP thousands separator. */
export const nf = (n: number) => n.toLocaleString("ja-JP");

/** Two-digit zero padding ("5" -> "05"). */
export const pad = (n: number) => String(n).padStart(2, "0");

/** Seconds -> "3h 12m" (or "45m" under an hour). */
export function fmtDur(seconds: number): string {
  const s = Math.max(0, Math.floor(seconds));
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  return h > 0 ? `${h}h ${pad(m)}m` : `${m}m`;
}

const KB = 1024;
const MB = KB * 1024;
const GB = MB * 1024;

/**
 * Bytes -> "12 MB" with 1024-based units. `range` picks the units and rounding of a screen:
 * - "mb": always whole MB, no thousands separator (can be "0 MB").
 * - "gb": whole MB (at least 1), switching to one-decimal GB from 1 GB.
 * - "kb": whole KB (at least 1), switching to one-decimal MB from 1 MB.
 */
export function fmtSize(bytes: number, range: "mb" | "gb" | "kb"): string {
  if (range === "mb") return `${Math.round(bytes / MB)} MB`;
  if (range === "gb") return bytes >= GB ? `${(bytes / GB).toFixed(1)} GB` : `${nf(Math.max(1, Math.round(bytes / MB)))} MB`;
  return bytes >= MB ? `${nf(Math.round((bytes / MB) * 10) / 10)} MB` : `${nf(Math.max(1, Math.round(bytes / KB)))} KB`;
}
