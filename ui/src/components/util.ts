export function errText(e: unknown): string {
  if (typeof e === "string") return e;
  if (e instanceof Error) return e.message;
  return String(e);
}

/** One decimal, null -> empty. */
export function fmtMb(x: number | null | undefined): string {
  return x == null ? "" : x.toFixed(1);
}

export function numText(x: number | null | undefined): string {
  return x == null ? "" : String(x);
}

export function nullIfBlank(s: string): string | null {
  const t = s.trim();
  return t === "" ? null : t;
}

/** Selected value of a kind/level select -> null when "(全て)". */
export function nullIfAll(s: string, all: string): string | null {
  return s === all || s === "" ? null : s;
}

/** Interface every refreshable tab exposes to App. */
export interface TabHandle {
  /** Reload from the first page, keeping the selection where possible. */
  refresh(): void;
  /** Number of rows currently loaded (paged tabs only). */
  loadedCount?(): number;
  /** Snapshot of what the tab shows, for Back / Forward (JSON-comparable). */
  getView?(): unknown;
  /** Show a snapshot taken by `getView` again. */
  restoreView?(view: unknown): void;
}
