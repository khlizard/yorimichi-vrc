import { useRef } from "react";
import { AUTO_REFRESH_TABS, TABS, type TabRefs } from "./appTabs";

/** Reloading the tabs: all of them, or only what an automatic import calls for (see TABS). */
export function useTabRefresh(refs: TabRefs) {
  /** An automatic import changed data but a paged list (players / logs) was left alone. */
  const pending = useRef(false);

  const refreshAll = () => {
    pending.current = false;
    for (const t of TABS) refs[t.id].current?.refresh();
  };

  /** After an automatic import. Returns true while some list is still stale (see `pending`). */
  const refreshAfterAutoImport = () => {
    for (const t of AUTO_REFRESH_TABS) {
      const h = refs[t.id].current;
      if (t.autoRefresh === "always") h?.refresh();
      else if ((h?.loadedCount?.() ?? 0) <= (t.pageSize ?? 0)) h?.refresh();
      else pending.current = true;
    }
    return pending.current;
  };

  return { refreshAll, refreshAfterAutoImport, hasPending: () => pending.current };
}
