/** One Back / Forward step: a tab and what it showed (see TabHandle.getView). */
export interface HistEntry<T extends string = string> {
  tab: T;
  view: unknown;
}

export const HISTORY_MAX = 50;

export const sameEntry = (a: HistEntry, b: HistEntry) => a.tab === b.tab && JSON.stringify(a.view) === JSON.stringify(b.view);

export interface HistoryStacks<E> {
  back: E[];
  forward: E[];
}

export const emptyHistory = <E>(): HistoryStacks<E> => ({ back: [], forward: [] });

/**
 * A new move: remember `before` (unless it equals the top of the back stack), keep at most `max`
 * entries (the oldest drop out) and forget everything that could be gone Forward to.
 */
export function recordMove<E>(s: HistoryStacks<E>, before: E, same: (a: E, b: E) => boolean, max = HISTORY_MAX): HistoryStacks<E> {
  let back = s.back;
  const top = back[back.length - 1];
  if (top === undefined || !same(top, before)) {
    back = [...back, before];
    if (back.length > max) back = back.slice(back.length - max);
  }
  return { back, forward: [] };
}

/**
 * Back (-1) / Forward (+1): pop the entry to go to and push `snapshot()` (where we are now) onto the
 * opposite stack. null when there is nothing to go to; `snapshot` is not called then.
 */
export function stepHistory<E>(s: HistoryStacks<E>, dir: -1 | 1, snapshot: () => E): { stacks: HistoryStacks<E>; entry: E } | null {
  const from = dir === -1 ? s.back : s.forward;
  if (from.length === 0) return null;
  const entry = from[from.length - 1];
  const rest = from.slice(0, -1);
  const to = [...(dir === -1 ? s.forward : s.back), snapshot()];
  return { stacks: dir === -1 ? { back: rest, forward: to } : { back: to, forward: rest }, entry };
}
