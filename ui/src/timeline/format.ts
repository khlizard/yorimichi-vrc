/** Seconds -> "3h 12m" (or "45m" under an hour). */
export function fmtDur(seconds: number): string {
  const s = Math.max(0, Math.floor(seconds));
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  return h > 0 ? `${h}h ${String(m).padStart(2, "0")}m` : `${m}m`;
}

export const nf = (n: number) => n.toLocaleString("ja-JP");
