import { useEffect, useMemo, useRef, useState } from "react";
import { flushSync } from "react-dom";
import {
  PLAYER_PAGE_SIZE,
  RAW_PAGE_SIZE,
  answerTrayChoice,
  applyUpdate,
  getSettings,
  getStatus,
  importNow,
  onAskTray,
  onImportFinished,
  onImporting,
  onSettingsChanged,
  onStatus,
  onUpdateReady,
  type Settings,
} from "./api";
import { DialogProvider, useDialog } from "./components/Dialog";
import { errText, type TabHandle } from "./components/util";
import { AvatarsTab } from "./tabs/AvatarsTab";
import { LogsTab } from "./tabs/LogsTab";
import { PhotoNavContext, type PhotoFilter } from "./photos/nav";
import { PhotosTab, type PhotosHandle } from "./photos/PhotosTab";
import { NotificationsTab } from "./tabs/NotificationsTab";
import { PlayersTab, type PlayersHandle } from "./tabs/PlayersTab";
import { SessionsTab, type SessionsHandle } from "./tabs/SessionsTab";
import { UrlsTab } from "./tabs/UrlsTab";

const TABS = [
  { id: "photos", label: "写真" },
  { id: "players", label: "プレイヤー" },
  { id: "sessions", label: "セッション" },
  { id: "urls", label: "URL" },
  { id: "avatars", label: "アバター" },
  { id: "notifications", label: "通知" },
  { id: "logs", label: "生ログ" },
] as const;
type TabId = (typeof TABS)[number]["id"];

const PENDING_STATUS = "新しいログを取り込みました（続きを読み込んだ一覧は「再取り込み」で更新）";

export default function App() {
  return (
    <DialogProvider>
      <Main />
    </DialogProvider>
  );
}

function Main() {
  const { showError, choose } = useDialog();
  const [tab, setTab] = useState<TabId>("players");
  const [from, setFrom] = useState<string | null>(null);
  const [to, setTo] = useState<string | null>(null);
  const [status, setStatus] = useState("");
  const [importing, setImporting] = useState(false);
  // Latest settings, kept for features that depend on them (badges, day boundary).
  const [settings, setSettings] = useState<Settings | null>(null);
  const askingTray = useRef(false);
  const chooseRef = useRef(choose);
  chooseRef.current = choose;

  // Imperative handles of every refreshable tab. Refresh is explicit: App calls handle.refresh().
  const photos = useRef<PhotosHandle>(null);
  const players = useRef<PlayersHandle>(null);
  const sessions = useRef<SessionsHandle>(null);
  const urls = useRef<TabHandle>(null);
  const avatars = useRef<TabHandle>(null);
  const notifications = useRef<TabHandle>(null);
  const logs = useRef<TabHandle>(null);
  /** An automatic import changed data but a paged list (players/logs) was left alone. */
  const pending = useRef(false);

  const refreshAll = () => {
    pending.current = false;
    for (const h of [photos, players, sessions, urls, avatars, notifications, logs]) h.current?.refresh();
  };
  const refreshAllRef = useRef(refreshAll);
  refreshAllRef.current = refreshAll;

  // Date change (and first mount) -> refresh all tabs. Tabs read the latest from/to when refreshing.
  useEffect(() => {
    refreshAllRef.current();
  }, [from, to]);

  useEffect(() => {
    let alive = true;
    const unlisten: Promise<() => void>[] = [];
    getStatus()
      .then((s) => {
        if (!alive) return;
        setStatus((prev) => prev || s.status);
        setImporting(s.importing);
      })
      .catch((e) => {
        if (alive) setStatus(errText(e));
      });
    getSettings()
      .then((st) => {
        if (alive) setSettings(st);
      })
      .catch(() => {});
    unlisten.push(onStatus((s) => setStatus(s)));
    unlisten.push(
      onSettingsChanged((st) => {
        setSettings(st);
        refreshAllRef.current();
      }),
    );
    unlisten.push(
      onAskTray(() => {
        if (askingTray.current) return;
        askingTray.current = true;
        chooseRef
          .current({
            title: "閉じるときの動作",
            message: "ウィンドウを閉じたあとも、タスクトレイに残してログの取り込みを続けますか？ あとから設定で変更できます。",
            buttons: [
              { id: "stay", label: "トレイに残す", primary: true },
              { id: "exit", label: "終了する" },
              { id: "cancel", label: "キャンセル" },
            ],
            cancelId: "cancel",
          })
          .then((id) => {
            if (id === "stay" || id === "exit") return answerTrayChoice(id === "stay");
          })
          .catch((e) => showError(errText(e)))
          .finally(() => {
            askingTray.current = false;
          });
      }),
    );
    unlisten.push(
      onUpdateReady(({ version }) => {
        chooseRef
          .current({
            title: "更新",
            message: `新しいバージョン ${version} をダウンロードしました。再起動して更新しますか？`,
            buttons: [
              { id: "apply", label: "再起動して更新", primary: true },
              { id: "later", label: "あとで" },
            ],
            cancelId: "later",
          })
          .then((id) => {
            if (id === "apply") return applyUpdate();
          })
          .catch((e) => showError(errText(e)));
      }),
    );
    unlisten.push(onImporting((b) => setImporting(b)));
    unlisten.push(
      onImportFinished(({ changed, automatic }) => {
        if (changed && !automatic) {
          refreshAllRef.current();
        } else if (changed && automatic) {
          sessions.current?.refresh();
          urls.current?.refresh();
          avatars.current?.refresh();
          notifications.current?.refresh();
          if ((players.current?.loadedCount?.() ?? 0) <= PLAYER_PAGE_SIZE) players.current?.refresh();
          else pending.current = true;
          if ((logs.current?.loadedCount?.() ?? 0) <= RAW_PAGE_SIZE) logs.current?.refresh();
          else pending.current = true;
          if (pending.current) setStatus(PENDING_STATUS);
        } else if (!changed && !automatic && pending.current) {
          refreshAllRef.current();
        }
      }),
    );
    return () => {
      alive = false;
      unlisten.forEach((p) => p.then((f) => f()).catch(() => {}));
    };
  }, []);

  const navigateToVisit = (visitId: number | null) => {
    if (visitId == null) return;
    flushSync(() => setTab("sessions"));
    void sessions.current?.selectVisitById(visitId);
  };

  const navigateToPlayer = (_playerId: number, displayName: string) => {
    flushSync(() => setTab("players"));
    players.current?.searchFor(displayName);
  };

  const navigateToPhotos = (filter: PhotoFilter) => {
    flushSync(() => setTab("photos"));
    photos.current?.showFor(filter);
  };
  const navigateToPhotosRef = useRef(navigateToPhotos);
  navigateToPhotosRef.current = navigateToPhotos;
  // Stable context value so tabs using it do not re-render on every App render.
  const photoNav = useMemo(() => ({ showPhotos: (f: PhotoFilter) => navigateToPhotosRef.current(f) }), []);

  const onTabKey = (e: React.KeyboardEvent, i: number) => {
    let n = i;
    if (e.key === "ArrowRight") n = (i + 1) % TABS.length;
    else if (e.key === "ArrowLeft") n = (i + TABS.length - 1) % TABS.length;
    else return;
    e.preventDefault();
    setTab(TABS[n].id);
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
    <PhotoNavContext.Provider value={photoNav}>
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
            onClick={() => setTab(t.id)}
            onKeyDown={(e) => onTabKey(e, i)}
          >
            {t.label}
          </button>
        ))}
      </div>
      <div className="tabbody">
        {panel(
          "photos",
          <PhotosTab
            ref={photos}
            from={from}
            to={to}
            active={tab === "photos"}
            dayStartHour={settings?.DayStartHour ?? 0}
            photoFolders={settings?.PhotoFolders ?? []}
            onNavigateVisit={navigateToVisit}
            onNavigatePlayer={navigateToPlayer}
          />,
        )}
        {panel("players", <PlayersTab ref={players} from={from} to={to} onNavigate={navigateToVisit} />)}
        {panel("sessions", <SessionsTab ref={sessions} from={from} to={to} onNavigatePlayer={navigateToPlayer} />)}
        {panel("urls", <UrlsTab ref={urls} from={from} to={to} onNavigate={navigateToVisit} />)}
        {panel("avatars", <AvatarsTab ref={avatars} from={from} to={to} onNavigate={navigateToVisit} />)}
        {panel("notifications", <NotificationsTab ref={notifications} from={from} to={to} onNavigate={navigateToVisit} />)}
        {panel("logs", <LogsTab ref={logs} from={from} to={to} onNavigate={navigateToVisit} />)}
      </div>
    </div>
    </PhotoNavContext.Provider>
  );
}
