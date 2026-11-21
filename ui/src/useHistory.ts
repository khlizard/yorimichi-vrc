import { useEffect, useRef, useState } from "react";
import { flushSync } from "react-dom";
import { useLatest } from "./components/hooks";
import { emptyHistory, recordMove, sameEntry, stepHistory, type HistEntry, type HistoryStacks } from "./history";
import { isShown, runBackIntercepts } from "./nav";

/** A modal dialog is open: Back / Forward do nothing then. */
const modalOpen = () => Array.from(document.querySelectorAll(".modal-backdrop")).some(isShown);

interface Options<T extends string> {
  initialTab: T;
  /** What the tab shows now (TabHandle.getView), for the entry we leave. */
  getView(tab: T): unknown;
  /** Show a remembered view again (TabHandle.restoreView). */
  restoreView(tab: T, view: unknown): void;
}

/**
 * The current tab plus Back / Forward history across tabs.
 * `go` is the one way to move to a screen; `move` steps through the history.
 */
export function useHistory<T extends string>(opts: Options<T>) {
  const [tab, setTab] = useState<T>(opts.initialTab);
  const tabRef = useRef<T>(opts.initialTab);
  const stacks = useRef<HistoryStacks<HistEntry<T>>>(emptyHistory());
  const [, setHistoryVersion] = useState(0);
  const o = useLatest(opts);

  const snapshot = (): HistEntry<T> => ({ tab: tabRef.current, view: o.current.getView(tabRef.current) });
  const showTab = (t: T) => {
    tabRef.current = t;
    flushSync(() => setTab(t));
  };

  /** Every move between screens goes through here: remember where we are, then switch and apply. */
  const go = (t: T, apply?: () => void) => {
    stacks.current = recordMove(stacks.current, snapshot(), sameEntry);
    showTab(t);
    apply?.();
    setHistoryVersion((v) => v + 1);
  };

  /** Back (-1) / Forward (+1). A lightbox closes first; nothing happens while a dialog is open. */
  const move = (dir: -1 | 1) => {
    if (modalOpen()) return;
    if (dir === -1 && runBackIntercepts()) return;
    const r = stepHistory(stacks.current, dir, snapshot);
    if (!r) return;
    stacks.current = r.stacks;
    showTab(r.entry.tab);
    if (r.entry.view !== undefined) o.current.restoreView(r.entry.tab, r.entry.view);
    setHistoryVersion((v) => v + 1);
  };
  const moveRef = useLatest(move);

  // Alt+← / Alt+→, the keyboard's Back / Forward keys and the mouse's back / forward buttons.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const plainAlt = e.altKey && !e.ctrlKey && !e.shiftKey && !e.metaKey;
      const back = (plainAlt && e.key === "ArrowLeft") || e.key === "BrowserBack";
      const forward = (plainAlt && e.key === "ArrowRight") || e.key === "BrowserForward";
      if (!back && !forward) return;
      e.preventDefault();
      e.stopPropagation();
      moveRef.current(back ? -1 : 1);
    };
    const onMouse = (e: MouseEvent) => {
      if (e.button !== 3 && e.button !== 4) return;
      e.preventDefault();
      if (e.type === "mouseup") moveRef.current(e.button === 3 ? -1 : 1);
    };
    window.addEventListener("keydown", onKey, true);
    window.addEventListener("mousedown", onMouse, true);
    window.addEventListener("mouseup", onMouse, true);
    return () => {
      window.removeEventListener("keydown", onKey, true);
      window.removeEventListener("mousedown", onMouse, true);
      window.removeEventListener("mouseup", onMouse, true);
    };
  }, [moveRef]);

  return {
    tab,
    /** The tab that is shown right now (updated synchronously, unlike `tab`). */
    tabRef,
    go,
    move,
    canBack: stacks.current.back.length > 0,
    canForward: stacks.current.forward.length > 0,
  };
}
