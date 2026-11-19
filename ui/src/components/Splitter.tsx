import { useRef } from "react";

/**
 * Drag handle. direction "col" = vertical bar between left/right panes (reports dx),
 * "row" = horizontal bar between top/bottom panes (reports dy).
 */
export function Splitter({ direction, onDelta, label }: { direction: "col" | "row"; onDelta: (d: number) => void; label: string }) {
  const last = useRef<number | null>(null);
  const pos = (e: React.PointerEvent) => (direction === "col" ? e.clientX : e.clientY);
  return (
    <div
      className={`splitter splitter-${direction}`}
      role="separator"
      aria-orientation={direction === "col" ? "vertical" : "horizontal"}
      aria-label={label}
      tabIndex={0}
      onPointerDown={(e) => {
        e.currentTarget.setPointerCapture(e.pointerId);
        last.current = pos(e);
      }}
      onPointerMove={(e) => {
        if (last.current == null) return;
        const p = pos(e);
        const d = p - last.current;
        if (d !== 0) {
          last.current = p;
          onDelta(d);
        }
      }}
      onPointerUp={() => {
        last.current = null;
      }}
      onPointerCancel={() => {
        last.current = null;
      }}
      onKeyDown={(e) => {
        const step = 16;
        if (direction === "col") {
          if (e.key === "ArrowLeft") onDelta(-step);
          else if (e.key === "ArrowRight") onDelta(step);
        } else {
          if (e.key === "ArrowUp") onDelta(-step);
          else if (e.key === "ArrowDown") onDelta(step);
        }
      }}
    />
  );
}
