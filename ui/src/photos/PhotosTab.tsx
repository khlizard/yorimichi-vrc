import { useEffect, useImperativeHandle, useMemo, useRef, useState, type Ref } from "react";
import {
  PHOTO_PAGE_SIZE,
  onPhotosChanged,
  photoDays,
  photos as fetchPhotos,
  rescanPhotos,
  type DayCount,
  type PhotoRow,
} from "../api";
import { useDialog } from "../components/Dialog";
import { useLatest } from "../components/hooks";
import { errText, type TabHandle } from "../components/util";
import { Lightbox } from "./Lightbox";
import { PhotoGrid, type GridSize, type PhotoGridHandle } from "./PhotoGrid";
import type { PhotoFilter } from "./nav";
import "./photos.css";

export type { PhotoFilter } from "./nav";

export interface PhotosHandle extends TabHandle {
  /** Show only the photos of a world / player, with a removable chip. */
  showFor(filter: PhotoFilter): void;
}

const SIZE_KEY = "photoGridSize";
const SIZES: { id: GridSize; label: string }[] = [
  { id: "s", label: "小" },
  { id: "m", label: "中" },
  { id: "l", label: "大" },
];
const pad = (n: number) => String(n).padStart(2, "0");

function loadSize(): GridSize {
  try {
    const v = localStorage.getItem(SIZE_KEY);
    if (v === "s" || v === "m" || v === "l") return v;
  } catch {
    // storage unavailable
  }
  return "m";
}

/** First instant after the logical month "yyyy-MM" (a logical day starts at dayStart). */
function monthEnd(month: string, dayStart: number): string {
  const y = Number(month.slice(0, 4));
  const m = Number(month.slice(5, 7));
  const ny = m === 12 ? y + 1 : y;
  const nm = m === 12 ? 1 : m + 1;
  return `${ny}-${pad(nm)}-01 ${pad(dayStart)}:00:00`;
}

function monthsOf(days: DayCount[]): { key: string; label: string; count: number }[] {
  const out: { key: string; label: string; count: number }[] = [];
  for (const d of days) {
    const key = d.day.slice(0, 7);
    const last = out[out.length - 1];
    if (last && last.key === key) last.count += d.count;
    else out.push({ key, label: `${Number(key.slice(0, 4))}年${Number(key.slice(5, 7))}月`, count: d.count });
  }
  return out;
}

export function PhotosTab({
  from,
  to,
  active,
  dayStartHour,
  photoFolders,
  onNavigateVisit,
  onNavigatePlayer,
  ref,
}: {
  from: string | null;
  to: string | null;
  /** The tab is shown; nothing is loaded before the first time. */
  active: boolean;
  dayStartHour: number;
  photoFolders: string[];
  onNavigateVisit: (visitId: number | null) => void;
  onNavigatePlayer: (playerId: number, displayName: string) => void;
  ref?: Ref<PhotosHandle>;
}) {
  const { showError } = useDialog();
  const range = useLatest({ from, to });
  const dayStart = useLatest(dayStartHour);
  const grid = useRef<PhotoGridHandle>(null);
  const gen = useRef(0);
  const activated = useRef(false);

  const [items, setItemsState] = useState<PhotoRow[]>([]);
  const itemsRef = useRef<PhotoRow[]>([]);
  const [hasMore, setHasMore] = useState(false);
  const [loaded, setLoaded] = useState(false);
  const [days, setDays] = useState<DayCount[]>([]);
  const [favOnly, setFavOnlyState] = useState(false);
  const favRef = useRef(false);
  const [filter, setFilterState] = useState<PhotoFilter | null>(null);
  const filterRef = useRef<PhotoFilter | null>(null);
  const [month, setMonthState] = useState<string | null>(null);
  const monthRef = useRef<string | null>(null);
  const [size, setSizeState] = useState<GridSize>(loadSize);
  const [notice, setNotice] = useState(false);
  const [rescanning, setRescanning] = useState(false);
  const [lb, setLbState] = useState<number | null>(null);
  const lbRef = useRef<number | null>(null);

  const setItems = (l: PhotoRow[]) => {
    itemsRef.current = l;
    setItemsState(l);
  };
  const setLb = (i: number | null) => {
    lbRef.current = i;
    setLbState(i);
  };
  const setFavOnly = (b: boolean) => {
    favRef.current = b;
    setFavOnlyState(b);
  };
  const setFilter = (f: PhotoFilter | null) => {
    filterRef.current = f;
    setFilterState(f);
  };
  const setMonth = (m: string | null) => {
    monthRef.current = m;
    setMonthState(m);
  };

  const base = () => ({
    from: range.current.from,
    to: range.current.to,
    worldRefId: filterRef.current?.worldRefId ?? null,
    playerId: filterRef.current?.playerId ?? null,
    favoriteOnly: favRef.current,
  });

  const reload = async (resetScroll = false) => {
    const g = ++gen.current;
    const q = base();
    const m = monthRef.current;
    setNotice(false);
    try {
      const [page, d] = await Promise.all([
        fetchPhotos({
          ...q,
          beforeTime: m ? monthEnd(m, dayStart.current) : null,
          // 0 keeps photos taken exactly at the boundary out of the page.
          beforeId: m ? 0 : null,
          limit: PHOTO_PAGE_SIZE,
        }),
        photoDays(q.from, q.to, q.worldRefId, q.playerId, q.favoriteOnly),
      ]);
      if (g !== gen.current) return;
      setItems(page);
      setHasMore(page.length === PHOTO_PAGE_SIZE);
      setDays(d);
      setLoaded(true);
      setLb(null);
      if (resetScroll) grid.current?.scrollToTop();
    } catch (e) {
      if (g === gen.current) showError(errText(e));
    }
  };
  const reloadRef = useLatest(reload);

  const loadingMore = useRef(false);
  const loadMore = async () => {
    const g = gen.current;
    const last = itemsRef.current[itemsRef.current.length - 1];
    if (!last || loadingMore.current) return;
    loadingMore.current = true;
    try {
      const page = await fetchPhotos({
        ...base(),
        beforeTime: last.takenAt,
        beforeId: last.id,
        limit: PHOTO_PAGE_SIZE,
      });
      if (g !== gen.current) return;
      setItems([...itemsRef.current, ...page]);
      setHasMore(page.length === PHOTO_PAGE_SIZE);
    } catch (e) {
      if (g === gen.current) showError(errText(e));
      throw e;
    } finally {
      loadingMore.current = false;
    }
  };

  // First time shown -> first load. Hidden tabs load nothing.
  useEffect(() => {
    if (active && !activated.current) {
      activated.current = true;
      void reload(true);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [active]);

  useEffect(() => {
    const un = onPhotosChanged(() => {
      if (!activated.current) return;
      const firstPage = monthRef.current == null && itemsRef.current.length <= PHOTO_PAGE_SIZE && lbRef.current == null;
      if (firstPage) void reloadRef.current();
      else setNotice(true);
    });
    return () => {
      un.then((f) => f()).catch(() => {});
    };
  }, [reloadRef]);

  useImperativeHandle(ref, () => ({
    refresh: () => {
      if (activated.current) void reload();
    },
    loadedCount: () => itemsRef.current.length,
    showFor: (f: PhotoFilter) => {
      activated.current = true;
      setFilter(f);
      setMonth(null);
      void reload(true);
    },
  }));

  const months = useMemo(() => monthsOf(days), [days]);
  const total = useMemo(() => days.reduce((s, d) => s + d.count, 0), [days]);

  const changeSize = (s: GridSize) => {
    setSizeState(s);
    try {
      localStorage.setItem(SIZE_KEY, s);
    } catch {
      // storage unavailable
    }
  };

  const emptyText =
    photoFolders.length === 0
      ? "写真フォルダが設定されていません。メニューの「ファイル → 設定…」で指定してください。"
      : !loaded
        ? "読み込み中…"
        : "写真が見つかりませんでした。";

  const chipPrefix = filter?.playerId != null ? "プレイヤー" : "ワールド";

  return (
    <div className="photo-surface ps">
      <div className="ps-toolbar" role="toolbar" aria-label="写真の表示設定">
        <button
          type="button"
          aria-pressed={favOnly}
          onClick={() => {
            setFavOnly(!favRef.current);
            setMonth(null);
            void reload(true);
          }}
        >
          お気に入りのみ
        </button>
        <div className="ps-seg" role="group" aria-label="サムネイルの大きさ">
          {SIZES.map((s) => (
            <button key={s.id} type="button" aria-pressed={size === s.id} onClick={() => changeSize(s.id)}>
              {s.label}
            </button>
          ))}
        </div>
        <label className="ps-month">
          <span className="sr-only">月へ移動</span>
          <select
            aria-label="月へ移動"
            value={month ?? ""}
            onChange={(e) => {
              setMonth(e.target.value || null);
              void reload(true);
            }}
          >
            <option value="">月へ移動…</option>
            {months.map((m) => (
              <option key={m.key} value={m.key}>
                {m.label} ({m.count.toLocaleString("ja-JP")})
              </option>
            ))}
          </select>
        </label>
        {month && (
          <button
            type="button"
            onClick={() => {
              setMonth(null);
              void reload(true);
            }}
          >
            最新に戻る
          </button>
        )}
        {filter && (
          <button
            type="button"
            className="ps-chip"
            title="絞り込みを解除"
            aria-label={`${chipPrefix}: ${filter.label}（絞り込みを解除）`}
            onClick={() => {
              setFilter(null);
              void reload(true);
            }}
          >
            {chipPrefix}: {filter.label} ×
          </button>
        )}
        <span className="ps-spacer" />
        {loaded && <span className="ps-total">{total.toLocaleString("ja-JP")} 枚</span>}
        <button
          type="button"
          disabled={rescanning}
          onClick={() => {
            setRescanning(true);
            rescanPhotos()
              .catch((e) => showError(errText(e)))
              .finally(() => setRescanning(false));
          }}
        >
          写真を再読み込み
        </button>
      </div>
      {notice && (
        <div className="ps-notice" role="status">
          <span>新しい写真があります</span>
          <button type="button" onClick={() => void reload()}>
            更新
          </button>
        </div>
      )}
      <PhotoGrid
        ref={grid}
        items={items}
        dayStartHour={dayStartHour}
        size={size}
        active={active}
        hasMore={hasMore}
        loadMore={loadMore}
        onOpen={(i) => setLb(i)}
        emptyText={emptyText}
      />
      {lb !== null && (
        <Lightbox
          items={items}
          index={lb}
          onIndexChange={(i) => {
            setLb(i);
            grid.current?.scrollToIndex(i);
          }}
          onClose={() => setLb(null)}
          onNavigateVisit={(id) => {
            setLb(null);
            onNavigateVisit(id);
          }}
          onNavigatePlayer={(id, name) => {
            setLb(null);
            onNavigatePlayer(id, name);
          }}
          onFavoriteChanged={(id, fav) => setItems(itemsRef.current.map((x) => (x.id === id ? { ...x, favorite: fav } : x)))}
          onNearEnd={() => {
            if (hasMore) void loadMore().catch(() => {});
          }}
        />
      )}
    </div>
  );
}
