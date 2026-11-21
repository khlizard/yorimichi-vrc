import { createRef, useEffect, useMemo, useRef, useState } from "react";
import { importNow, type Settings } from "./api";
import { DialogProvider, useDialog } from "./components/Dialog";
import { MetadataProvider, useMetadataDialog } from "./features/metadata/MetadataProvider";
import { errText } from "./components/util";
import { AvatarsTab } from "./features/logs/AvatarsTab";
import { LogsTab } from "./features/logs/LogsTab";
import "./app-extra.css";
import { TABS, type TabId, type TabRefs } from "./appTabs";
import { AppNavContext, type AppNav, type PhotoFilter } from "./nav";
import { logicalDay } from "./features/photos/photoUtil";
import { AlbumsTab } from "./features/albums/AlbumsTab";
import { PhotosTab } from "./features/photos/PhotosTab";
import { NotificationsTab } from "./features/logs/NotificationsTab";
import { PlayersTab } from "./features/players/PlayersTab";
import { SessionsTab } from "./features/sessions/SessionsTab";
import { UrlsTab } from "./features/logs/UrlsTab";
import { StatsTab } from "./features/stats/StatsTab";
import { TimelineTab } from "./features/timeline/TimelineTab";
import { useBackendEvents } from "./useBackendEvents";
import { useHistory } from "./useHistory";
import { useTabRefresh } from "./useTabRefresh";
import { WorldsTab } from "./features/worlds/WorldsTab";

export default function App() {
  return (
    <DialogProvider>
      <MetadataProvider>
        <Main />
      </MetadataProvider>
    </DialogProvider>
  );
}

function Main() {
  const { showError, choose } = useDialog();
  const metadata = useMetadataDialog();
  const [from, setFrom] = useState<string | null>(null);
  const [to, setTo] = useState<string | null>(null);
  const [status, setStatus] = useState("");
  const [importing, setImporting] = useState(false);
  // Latest settings, kept for features that depend on them (badges, day boundary).
  const [settings, setSettings] = useState<Settings | null>(null);

  // Imperative handles of every tab (one per TABS entry). Refresh is explicit: App calls handle.refresh().
  const [refs] = useState<TabRefs>(() => ({
    timeline: createRef(),
    photos: createRef(),
    albums: createRef(),
    players: createRef(),
    sessions: createRef(),
    worlds: createRef(),
    urls: createRef(),
    avatars: createRef(),
    notifications: createRef(),
    logs: createRef(),
    stats: createRef(),
  }));

  const { tab, tabRef, go, move, canBack, canForward } = useHistory<TabId>({
    initialTab: "timeline",
    getView: (t) => refs[t].current?.getView?.(),
    restoreView: (t, view) => refs[t].current?.restoreView?.(view),
  });

  const { refreshAll, refreshAfterAutoImport, hasPending } = useTabRefresh(refs);
  const refreshAllRef = useRef(refreshAll);
  refreshAllRef.current = refreshAll;

  // Date change (and first mount) -> refresh all tabs. Tabs read the latest from/to when refreshing.
  useEffect(() => {
    refreshAllRef.current();
  }, [from, to]);

  useBackendEvents({
    setStatus,
    setImporting,
    setSettings,
    showError,
    choose,
    openMetadata: metadata.open,
    refreshAll,
    refreshAfterAutoImport,
    hasPending,
  });

  const navigateToVisit = (visitId: number | null) => {
    if (visitId == null) return;
    go("sessions", () => void refs.sessions.current?.selectVisitById(visitId));
  };

  const navigateToPlayer = (playerId: number, displayName: string) => {
    go("players", () => refs.players.current?.searchFor(displayName, playerId));
  };

  const navigateToPhotos = (filter: PhotoFilter) => {
    go("photos", () => refs.photos.current?.showFor(filter));
  };
  const showDay = (day: string) => go("timeline", () => refs.timeline.current?.showDay(day));

  const appNavRef = useRef<AppNav>(null as unknown as AppNav);
  appNavRef.current = {
    showPhotos: navigateToPhotos,
    showVisit: navigateToVisit,
    showPlayer: navigateToPlayer,
    showDay,
    showDayAt: (ts) => showDay(logicalDay(ts, settings?.DayStartHour ?? 0)),
    showWorld: (id) => go("worlds", () => refs.worlds.current?.selectWorld(id)),
    showAlbum: (id) => go("albums", () => refs.albums.current?.selectAlbum(id)),
  };
  const appNav = useMemo<AppNav>(
    () => ({
      showPhotos: (f) => appNavRef.current.showPhotos(f),
      showVisit: (id) => appNavRef.current.showVisit(id),
      showPlayer: (id, n) => appNavRef.current.showPlayer(id, n),
      showDay: (d) => appNavRef.current.showDay(d),
      showDayAt: (ts) => appNavRef.current.showDayAt(ts),
      showWorld: (id) => appNavRef.current.showWorld(id),
      showAlbum: (id) => appNavRef.current.showAlbum(id),
    }),
    [],
  );

  const onTabKey = (e: React.KeyboardEvent, i: number) => {
    let n = i;
    if (e.key === "ArrowRight") n = (i + 1) % TABS.length;
    else if (e.key === "ArrowLeft") n = (i + TABS.length - 1) % TABS.length;
    else if (e.key === "Home") n = 0;
    else if (e.key === "End") n = TABS.length - 1;
    else return;
    e.preventDefault();
    if (TABS[n].id !== tabRef.current) go(TABS[n].id);
    document.getElementById(`tab-${TABS[n].id}`)?.focus();
  };

  const panel = (id: TabId, content: React.ReactNode) => (
    <div
      key={id}
      id={`panel-${id}`}
      role="tabpanel"
      aria-labelledby={`tab-${id}`}
      className={`tabpanel${tab === id ? " active" : ""}`}
    >
      {content}
    </div>
  );

  return (
    <AppNavContext.Provider value={appNav}>
    <div className="app">
      <div className="toolbar">
        <button disabled={importing} onClick={() => importNow().catch((e) => showError(errText(e)))}>
          再取り込み
        </button>
        <label htmlFor="date-from" className="period-label">
          期間:
        </label>
        <input id="date-from" type="date" style={{ width: 130 }} value={from ?? ""} onChange={(e) => setFrom(e.target.value || null)} />
        <span>〜</span>
        <input id="date-to" type="date" aria-label="期間の終了日" style={{ width: 130 }} value={to ?? ""} onChange={(e) => setTo(e.target.value || null)} />
        <button
          onClick={() => {
            setFrom(null);
            setTo(null);
          }}
        >
          クリア
        </button>
        <div className="status ellipsis" title={status} role="status">
          {status}
        </div>
      </div>
      <div className="tabbar">
      <div className="histnav">
        <button type="button" aria-label="戻る" title="戻る (Alt+←)" disabled={!canBack} onClick={() => move(-1)}>
          ←
        </button>
        <button type="button" aria-label="進む" title="進む (Alt+→)" disabled={!canForward} onClick={() => move(1)}>
          →
        </button>
      </div>
      <div className="tabstrip" role="tablist" aria-label="画面">
        {TABS.map((t, i) => (
          <button
            key={t.id}
            id={`tab-${t.id}`}
            role="tab"
            aria-selected={tab === t.id}
            aria-controls={`panel-${t.id}`}
            tabIndex={tab === t.id ? 0 : -1}
            className={`tab${tab === t.id ? " active" : ""}`}
            onClick={() => t.id !== tabRef.current && go(t.id)}
            onKeyDown={(e) => onTabKey(e, i)}
          >
            {t.label}
          </button>
        ))}
      </div>
      </div>
      <div className="tabbody">
        {panel("timeline", <TimelineTab ref={refs.timeline} active={tab === "timeline"} dayStartHour={settings?.DayStartHour ?? 0} />)}
        {panel(
          "photos",
          <PhotosTab
            ref={refs.photos}
            from={from}
            to={to}
            active={tab === "photos"}
            dayStartHour={settings?.DayStartHour ?? 0}
            photoFolders={settings?.PhotoFolders ?? []}
          />,
        )}
        {panel("albums", <AlbumsTab ref={refs.albums} active={tab === "albums"} dayStartHour={settings?.DayStartHour ?? 0} />)}
        {panel("players", <PlayersTab ref={refs.players} from={from} to={to} />)}
        {panel("sessions", <SessionsTab ref={refs.sessions} from={from} to={to} dayStartHour={settings?.DayStartHour ?? 0} />)}
        {panel("worlds", <WorldsTab ref={refs.worlds} active={tab === "worlds"} />)}
        {panel("urls", <UrlsTab ref={refs.urls} from={from} to={to} />)}
        {panel("avatars", <AvatarsTab ref={refs.avatars} from={from} to={to} />)}
        {panel("notifications", <NotificationsTab ref={refs.notifications} from={from} to={to} />)}
        {panel("logs", <LogsTab ref={refs.logs} from={from} to={to} />)}
        {panel("stats", <StatsTab ref={refs.stats} active={tab === "stats"} from={from} to={to} />)}
      </div>
    </div>
    </AppNavContext.Provider>
  );
}
