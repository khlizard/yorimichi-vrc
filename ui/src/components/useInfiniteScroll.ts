import { useEffect, useRef, type RefObject } from "react";

const THRESHOLD = 5;
const SETTLE_MS = 80;

export interface InfiniteOptions {
  hasMore: boolean;
  /** Must resolve when the page has been loaded (and state set). */
  loadMore: () => Promise<void>;
  /** Number of loaded rows; a change re-checks whether the viewport is still unfilled. */
  itemCount: number;
}

/**
 * Loads the next page when the scroller is at the bottom (checked after scrolling settles),
 * with an in-flight guard. After a page loads, loads another only if the content still
 * does not fill the viewport.
 */
export function useInfiniteScroll(ref: RefObject<HTMLElement | null>, opts: InfiniteOptions | undefined) {
  const optsRef = useRef(opts);
  optsRef.current = opts;
  const inflight = useRef(false);
  const timer = useRef<number | undefined>(undefined);

  const run = useRef((onlyIfUnfilled: boolean) => {
    const o = optsRef.current;
    const el = ref.current;
    if (!o || !el || inflight.current || !o.hasMore) return;
    if (el.clientHeight === 0) return; // not laid out
    const atBottom = el.scrollTop + el.clientHeight >= el.scrollHeight - THRESHOLD;
    const unfilled = el.scrollHeight <= el.clientHeight;
    if (onlyIfUnfilled ? !unfilled : !atBottom) return;
    inflight.current = true;
    // On failure do not retry automatically (avoids an error loop); success re-checks fill.
    o.loadMore().then(
      () => {
        inflight.current = false;
        window.setTimeout(() => run.current(true), SETTLE_MS);
      },
      () => {
        inflight.current = false;
      },
    );
  });

  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const onScroll = () => {
      window.clearTimeout(timer.current);
      timer.current = window.setTimeout(() => run.current(false), SETTLE_MS);
    };
    el.addEventListener("scroll", onScroll, { passive: true });
    const ro = new ResizeObserver(() => run.current(true));
    ro.observe(el);
    return () => {
      el.removeEventListener("scroll", onScroll);
      ro.disconnect();
      window.clearTimeout(timer.current);
    };
  }, [ref]);

  const count = opts?.itemCount;
  const hasMore = opts?.hasMore;
  useEffect(() => {
    const id = window.setTimeout(() => run.current(true), SETTLE_MS);
    return () => window.clearTimeout(id);
  }, [count, hasMore]);
}
