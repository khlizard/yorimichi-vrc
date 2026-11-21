import { useVirtualizer } from "@tanstack/react-virtual";
import { memo, useCallback, useEffect, useImperativeHandle, useLayoutEffect, useMemo, useRef, useState, type MouseEvent, type Ref } from "react";
import { type PhotoRow } from "../../api";
import type { MenuItem } from "../../components/ContextMenu";
import { useLatest } from "../../components/hooks";
import { buildLayout, GAP } from "./layout";
import { photoLabel } from "./photoUtil";
import { Thumb } from "./Thumb";
import { usePhotoMenu } from "./usePhotoMenu";
import { StarIcon } from "../../components/Star";
import { nf } from "../../format";
import "./photos.css";

export type GridSize = "s" | "m" | "l";

const TARGET: Record<GridSize, number> = { s: 110, m: 170, l: 250 };

export interface PhotoGridHandle {
  scrollToTop(): void;
  /** Bring the photo at this item index into view. */
  scrollToIndex(index: number): void;
}

export interface PhotoGridProps {
  items: PhotoRow[];
  dayStartHour: number;
  groupByDay?: boolean;
  size: GridSize;
  hasMore?: boolean;
  loadMore?: () => Promise<void> | void;
  onOpen: (index: number) => void;
  selectable?: boolean;
  selectedIds?: Set<number>;
  /** shiftRange: ids from the last toggled item to this one (inclusive) on Shift+click. */
  onToggleSelect?: (id: number, shiftRange?: number[]) => void;
  emptyText: string;
  onFavoriteChanged?: (id: number, favorite: boolean) => void;
  /** Location-specific items added to the right-click menu. */
  menuExtra?: (photo: PhotoRow) => MenuItem[];
  /** false while the surrounding tab is hidden: resize events are ignored (default true). */
  active?: boolean;
  ref?: Ref<PhotoGridHandle>;
}

function Check() {
  return (
    <svg viewBox="0 0 16 16" width="12" height="12" aria-hidden="true">
      <path fill="none" stroke="currentColor" strokeWidth="2" d="m3.5 8.5 3 3 6-7" />
    </svg>
  );
}

interface TileProps {
  p: PhotoRow;
  index: number;
  w: number;
  h: number;
  selected: boolean;
  selectable: boolean;
  onClick: (index: number, shift: boolean) => void;
  onContextMenu: (e: MouseEvent, index: number) => void;
}

const PhotoTile = memo(function PhotoTile({ p, index, w, h, selected, selectable, onClick, onContextMenu }: TileProps) {
  return (
    <button
      type="button"
      className={`pg-tile${selected ? " selected" : ""}`}
      style={{ width: w, height: h }}
      data-photo-index={index}
      data-photo-id={p.id}
      aria-label={photoLabel(p)}
      aria-pressed={selectable ? selected : undefined}
      onClick={(e) => onClick(index, e.shiftKey)}
      onContextMenu={(e) => onContextMenu(e, index)}
    >
      <Thumb photo={p} />
      {selectable && (
        <span className={`pg-check${selected ? " on" : ""}`} aria-hidden="true">
          {selected && <Check />}
        </span>
      )}
      {p.favorite && (
        <span className="pg-fav" aria-label="お気に入り" role="img">
          <StarIcon filled size={14} />
        </span>
      )}
    </button>
  );
});

export function PhotoGrid(p: PhotoGridProps) {
  const { items, dayStartHour, groupByDay = true, size, active = true } = p;
  const props = useLatest(p);
  const activeRef = useLatest(active);
  const scrollRef = useRef<HTMLDivElement>(null);
  const [width, setWidth] = useState(0);
  const lastToggled = useRef<number | null>(null);
  const anchor = useRef(0);
  /** Layout top of the anchor row at the last scroll / layout; used to keep the view in place. */
  const anchorTop = useRef(0);
  const inflight = useRef(false);

  useEffect(() => {
    const el = scrollRef.current;
    if (!el) return;
    const measure = () => {
      if (!activeRef.current) return;
      setWidth((w) => (w === el.clientWidth ? w : el.clientWidth));
    };
    measure();
    const ro = new ResizeObserver(measure);
    ro.observe(el);
    return () => ro.disconnect();
  }, [activeRef]);

  // Catch up with size changes that happened while hidden.
  useEffect(() => {
    const el = scrollRef.current;
    if (active && el) setWidth((w) => (w === el.clientWidth ? w : el.clientWidth));
  }, [active]);

  const layout = useMemo(
    () => buildLayout(items, width, TARGET[size], dayStartHour, groupByDay),
    [items, width, size, dayStartHour, groupByDay],
  );
  const layoutRef = useLatest(layout);

  const v = useVirtualizer({
    count: layout.rows.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: (i) => layout.rows[i]?.height ?? 0,
    overscan: 4,
    getItemKey: (i) => layout.rows[i]?.key ?? i,
  });

  // Row sizes are exact (no measuring): drop cached sizes whenever the layout changes,
  // and keep the first visible photo in place.
  useLayoutEffect(() => {
    v.measure();
    const el = scrollRef.current;
    const row = layout.rowOfItem.length > anchor.current ? layout.rows[layout.rowOfItem[anchor.current]] : undefined;
    if (el && row) {
      const delta = row.top - anchorTop.current;
      if (el.scrollTop > 0 && Math.abs(delta) > 0) el.scrollTop += delta;
      anchorTop.current = row.top;
    }
  }, [layout, v]);

  const checkMore = useCallback(() => {
    const P = props.current;
    const el = scrollRef.current;
    if (!el || inflight.current || !P.hasMore || !P.loadMore || !activeRef.current) return;
    if (el.clientHeight === 0) return;
    if (el.scrollHeight - el.scrollTop - el.clientHeight > el.clientHeight * 1.5) return;
    inflight.current = true;
    // On failure do not retry until the list or the scroll position changes.
    Promise.resolve()
      .then(() => P.loadMore!())
      .then(
        () => {
          inflight.current = false;
        },
        () => {
          inflight.current = false;
        },
      );
  }, [props, activeRef]);

  useEffect(() => {
    checkMore();
  }, [items.length, p.hasMore, layout, active, checkMore]);

  useEffect(() => {
    const el = scrollRef.current;
    if (!el) return;
    let raf = 0;
    const onScroll = () => {
      if (raf) return;
      raf = requestAnimationFrame(() => {
        raf = 0;
        const { rows } = layoutRef.current;
        // Anchor = first photo of the first visible row (binary search on row tops).
        let lo = 0;
        let hi = rows.length - 1;
        while (lo < hi) {
          const mid = (lo + hi + 1) >> 1;
          if (rows[mid].top <= el.scrollTop) lo = mid;
          else hi = mid - 1;
        }
        for (let r = lo; r < rows.length; r++) {
          const row = rows[r];
          if (row.kind === "row") {
            anchor.current = row.tiles[0].index;
            anchorTop.current = row.top;
            break;
          }
        }
        checkMore();
      });
    };
    el.addEventListener("scroll", onScroll, { passive: true });
    return () => {
      el.removeEventListener("scroll", onScroll);
      if (raf) cancelAnimationFrame(raf);
    };
  }, [layoutRef, checkMore]);

  const scrollToIndex = useCallback(
    (i: number) => {
      const r = layoutRef.current.rowOfItem[i];
      if (r !== undefined) v.scrollToIndex(r, { align: "auto" });
    },
    [layoutRef, v],
  );

  useImperativeHandle(
    p.ref,
    () => ({
      scrollToTop: () => {
        anchor.current = 0;
        anchorTop.current = 0;
        scrollRef.current?.scrollTo({ top: 0 });
      },
      scrollToIndex,
    }),
    [scrollToIndex],
  );

  const menu = usePhotoMenu();
  const openMenu = menu.onContextMenu;
  const handleContextMenu = useCallback(
    (e: MouseEvent, index: number) => {
      const P = props.current;
      const photo = P.items[index];
      if (!photo) return;
      openMenu(e, photo, {
        onOpen: () => P.onOpen(index),
        onFavoriteChanged: P.onFavoriteChanged,
        extra: P.menuExtra?.(photo),
      });
    },
    [props, openMenu],
  );

  const handleClick = useCallback(
    (index: number, shift: boolean) => {
      const P = props.current;
      if (!P.selectable) {
        P.onOpen(index);
        return;
      }
      let range: number[] | undefined;
      const last = lastToggled.current;
      if (shift && last != null) {
        range = P.items.slice(Math.min(last, index), Math.max(last, index) + 1).map((x) => x.id);
      }
      lastToggled.current = index;
      P.onToggleSelect?.(P.items[index].id, range);
    },
    [props],
  );

  const focusIndex = (n: number) => {
    scrollToIndex(n);
    let tries = 0;
    const go = () => {
      const el = scrollRef.current?.querySelector<HTMLElement>(`[data-photo-index="${n}"]`);
      if (el) el.focus();
      else if (++tries < 4) requestAnimationFrame(go);
    };
    requestAnimationFrame(go);
  };

  // Arrow keys move between photos (Tab alone cannot cross the virtualised rows).
  const onKeyDown = (e: React.KeyboardEvent) => {
    if (e.ctrlKey || e.metaKey || e.altKey) return;
    const t = (e.target as HTMLElement).closest<HTMLElement>("[data-photo-index]");
    if (!t) return;
    const i = Number(t.dataset.photoIndex);
    let n = -1;
    if (e.key === "ArrowLeft") n = i - 1;
    else if (e.key === "ArrowRight") n = i + 1;
    else if (e.key === "ArrowUp" || e.key === "ArrowDown") {
      const { rows, rowOfItem } = layout;
      const dir = e.key === "ArrowUp" ? -1 : 1;
      const cur = rows[rowOfItem[i]];
      if (cur?.kind === "row") {
        let x = 0;
        for (const tile of cur.tiles) {
          if (tile.index === i) {
            x += tile.w / 2;
            break;
          }
          x += tile.w + GAP;
        }
        let r = rowOfItem[i] + dir;
        while (r >= 0 && r < rows.length && rows[r].kind !== "row") r += dir;
        const target = rows[r];
        if (target?.kind === "row") {
          let best = Infinity;
          let cx = 0;
          for (const tile of target.tiles) {
            const d = Math.abs(cx + tile.w / 2 - x);
            if (d < best) {
              best = d;
              n = tile.index;
            }
            cx += tile.w + GAP;
          }
        }
      }
    } else return;
    e.preventDefault();
    if (n >= 0 && n < items.length) focusIndex(n);
  };

  const sel = p.selectedIds;

  return (
    <div ref={scrollRef} className="pg" role="group" aria-label="写真一覧" onKeyDown={onKeyDown}>
      {items.length === 0 ? (
        <div className="pg-empty">{p.emptyText}</div>
      ) : (
        <div className="pg-inner" style={{ height: v.getTotalSize() }}>
          {v.getVirtualItems().map((vi) => {
            const row = layout.rows[vi.index];
            if (!row) return null;
            return (
              <div
                key={vi.key}
                className={row.kind === "header" ? "pg-header" : "pg-row"}
                style={{ height: row.height, transform: `translateY(${vi.start}px)` }}
              >
                {row.kind === "header" ? (
                  <>
                    <h3 className="pg-day">{row.label}</h3>
                    <span className="pg-count">{nf(row.count)} 枚</span>
                  </>
                ) : (
                  row.tiles.map((t) => (
                    <PhotoTile
                      key={items[t.index].id}
                      p={items[t.index]}
                      index={t.index}
                      w={t.w}
                      h={t.h}
                      selected={sel?.has(items[t.index].id) ?? false}
                      selectable={p.selectable ?? false}
                      onClick={handleClick}
                      onContextMenu={handleContextMenu}
                    />
                  ))
                )}
              </div>
            );
          })}
        </div>
      )}
      {menu.element}
    </div>
  );
}
