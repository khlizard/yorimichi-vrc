import { useVirtualizer } from "@tanstack/react-virtual";
import {
  useCallback,
  useEffect,
  useImperativeHandle,
  useRef,
  useState,
  type CSSProperties,
  type Ref,
  type ReactNode,
} from "react";
import { ContextMenu, type MenuItem } from "./ContextMenu";
import { useInfiniteScroll, type InfiniteOptions } from "./useInfiniteScroll";

export type Key = string | number;

export interface ListHandle {
  scrollToIndex(index: number): void;
}

export interface VirtualListProps<T> {
  items: T[];
  getKey: (item: T, index: number) => Key;
  renderRow: (item: T, selected: boolean, index: number) => ReactNode;
  rowHeight: number;
  /** Rows have variable height (measured). rowHeight is then the estimate. */
  dynamic?: boolean;
  /** Uncontrolled mode: keep the selection when items change (use only with stable keys). */
  keepSelection?: boolean;
  /** Controlled selection. When undefined the list keeps its own (reset when items change). */
  selectedKey?: Key | null;
  onSelect?: (item: T | null) => void;
  onActivate?: (item: T) => void;
  /** Ctrl+C on the focused list with a selected row. */
  onCopy?: (item: T) => void;
  getMenu?: (item: T) => MenuItem[];
  infinite?: InfiniteOptions;
  /** Sticky header rendered above the rows (inside the scroller). */
  header?: ReactNode;
  headerHeight?: number;
  minWidth?: number;
  rowStyle?: CSSProperties;
  rowClassName?: (item: T, selected: boolean) => string;
  className?: string;
  ariaLabel: string;
  ref?: Ref<ListHandle>;
}

export function VirtualList<T>(p: VirtualListProps<T>) {
  const { items, getKey, rowHeight, dynamic, header, headerHeight = 0 } = p;
  const scrollRef = useRef<HTMLDivElement>(null);
  const [innerSel, setInnerSel] = useState<Key | null>(null);
  const controlled = p.selectedKey !== undefined;
  const selKey = controlled ? p.selectedKey! : innerSel;
  const [menu, setMenu] = useState<{ x: number; y: number; items: MenuItem[] } | null>(null);
  const closeMenu = useCallback(() => setMenu(null), []);

  useEffect(() => {
    if (!controlled && !p.keepSelection) setInnerSel(null);
  }, [items, controlled, p.keepSelection]);

  const v = useVirtualizer({
    count: items.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => rowHeight,
    overscan: 8,
    scrollMargin: headerHeight,
    scrollPaddingStart: headerHeight,
    getItemKey: (i) => getKey(items[i], i),
  });

  useImperativeHandle(p.ref,() => ({ scrollToIndex: (i: number) => v.scrollToIndex(i, { align: "auto" }) }), [v]);

  useInfiniteScroll(scrollRef, p.infinite);

  const select = (item: T | null, index?: number) => {
    if (!controlled) setInnerSel(item == null ? null : getKey(item, index ?? items.indexOf(item)));
    p.onSelect?.(item);
  };

  const selectedIndex = selKey == null ? -1 : items.findIndex((it, i) => getKey(it, i) === selKey);

  const onKeyDown = (e: React.KeyboardEvent) => {
    if (items.length === 0) return;
    if ((e.ctrlKey || e.metaKey) && (e.key === "c" || e.key === "C")) {
      if (p.onCopy && selectedIndex >= 0) {
        e.preventDefault();
        p.onCopy(items[selectedIndex]);
      }
      return;
    }
    const page = Math.max(1, Math.floor((scrollRef.current?.clientHeight ?? rowHeight * 10) / rowHeight) - 1);
    let next = selectedIndex;
    switch (e.key) {
      case "ArrowDown":
        next = selectedIndex < 0 ? 0 : Math.min(items.length - 1, selectedIndex + 1);
        break;
      case "ArrowUp":
        next = selectedIndex < 0 ? 0 : Math.max(0, selectedIndex - 1);
        break;
      case "PageDown":
        next = Math.min(items.length - 1, Math.max(0, selectedIndex) + page);
        break;
      case "PageUp":
        next = Math.max(0, Math.max(0, selectedIndex) - page);
        break;
      case "Home":
        next = 0;
        break;
      case "End":
        next = items.length - 1;
        break;
      default:
        return;
    }
    e.preventDefault();
    if (next !== selectedIndex) {
      select(items[next], next);
      v.scrollToIndex(next, { align: "auto" });
    }
  };

  const total = v.getTotalSize();

  return (
    <div
      ref={scrollRef}
      className={`vlist ${p.className ?? ""}`}
      role="listbox"
      aria-label={p.ariaLabel}
      tabIndex={0}
      onKeyDown={onKeyDown}
    >
      <div className="vlist-inner" style={{ minWidth: p.minWidth }}>
        {header}
        <div className="vlist-body" style={{ height: total }}>
          {v.getVirtualItems().map((vi) => {
            const item = items[vi.index];
            if (item === undefined) return null;
            const selected = vi.index === selectedIndex;
            const cls = p.rowClassName?.(item, selected) ?? "";
            return (
              <div
                key={vi.key}
                data-index={vi.index}
                ref={dynamic ? v.measureElement : undefined}
                role="option"
                aria-selected={selected}
                className={`vrow${selected ? " selected" : ""}${cls ? " " + cls : ""}`}
                style={{
                  ...p.rowStyle,
                  height: dynamic ? undefined : rowHeight,
                  minHeight: dynamic ? rowHeight : undefined,
                  transform: `translateY(${vi.start - headerHeight}px)`,
                }}
                onPointerDown={(e) => {
                  if (e.button === 0 || e.button === 2) select(item, vi.index);
                }}
                onDoubleClick={() => p.onActivate?.(item)}
                onContextMenu={(e) => {
                  if (!p.getMenu) return;
                  e.preventDefault();
                  scrollRef.current?.focus({ preventScroll: true });
                  setMenu({ x: e.clientX, y: e.clientY, items: p.getMenu(item) });
                }}
              >
                {p.renderRow(item, selected, vi.index)}
              </div>
            );
          })}
        </div>
      </div>
      {menu && <ContextMenu x={menu.x} y={menu.y} items={menu.items} onClose={closeMenu} />}
    </div>
  );
}
