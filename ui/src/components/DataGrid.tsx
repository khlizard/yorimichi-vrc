import type { ReactNode } from "react";
import { VirtualList, type VirtualListProps } from "./VirtualList";

export interface Column<T> {
  header: string;
  /** Fixed pixel width. */
  width?: number;
  /** Flex weight (relative share of the remaining width) when width is not given. */
  flex?: number;
  align?: "right";
  /** Plain text of the cell; also used as the tooltip. */
  text: (row: T) => string;
  /** Custom cell content (e.g. a link). */
  render?: (row: T) => ReactNode;
}

const FLEX_MIN = 80;
export const ROW_HEIGHT = 24;

export type DataGridProps<T> = Omit<VirtualListProps<T>, "renderRow" | "rowHeight" | "header" | "headerHeight" | "minWidth" | "rowStyle"> & {
  columns: Column<T>[];
};

export function DataGrid<T>({ columns, className, ...rest }: DataGridProps<T>) {
  const template = columns.map((c) => (c.width != null ? `${c.width}px` : `minmax(${FLEX_MIN}px, ${c.flex ?? 1}fr)`)).join(" ");
  const minWidth = columns.reduce((s, c) => s + (c.width ?? FLEX_MIN), 0);
  const header = (
    <div className="grid-header" style={{ gridTemplateColumns: template, height: ROW_HEIGHT }}>
      {columns.map((c, i) => (
        <div key={i} className={`cell${c.align === "right" ? " right" : ""}`}>
          {c.header}
        </div>
      ))}
    </div>
  );
  return (
    <VirtualList<T>
      {...rest}
      className={`grid ${className ?? ""}`}
      rowHeight={ROW_HEIGHT}
      header={header}
      headerHeight={ROW_HEIGHT}
      minWidth={minWidth}
      rowStyle={{ gridTemplateColumns: template }}
      renderRow={(row) =>
        columns.map((c, i) => (
          <div key={i} className={`cell${c.align === "right" ? " right" : ""}`} title={c.text(row)}>
            {c.render ? c.render(row) : c.text(row)}
          </div>
        ))
      }
    />
  );
}
