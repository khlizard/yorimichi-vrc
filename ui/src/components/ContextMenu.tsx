import { useEffect, useRef, type KeyboardEvent as ReactKeyboardEvent } from "react";

export interface MenuItem {
  label: string;
  hint?: string;
  disabled?: boolean;
  onClick: () => void;
}

export type MenuEntry = MenuItem | "separator";

export function ContextMenu({ x, y, items, onClose }: { x: number; y: number; items: MenuEntry[]; onClose: () => void }) {
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const el = ref.current;
    if (el) {
      const r = el.getBoundingClientRect();
      el.style.left = `${Math.max(0, Math.min(x, window.innerWidth - r.width - 2))}px`;
      el.style.top = `${Math.max(0, Math.min(y, window.innerHeight - r.height - 2))}px`;
    }
    const down = (e: PointerEvent) => {
      if (!ref.current?.contains(e.target as Node)) onClose();
    };
    const key = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      // Only the menu closes (not e.g. a lightbox it was opened on).
      e.preventDefault();
      e.stopPropagation();
      onClose();
    };
    window.addEventListener("pointerdown", down, true);
    window.addEventListener("keydown", key, true);
    window.addEventListener("blur", onClose);
    window.addEventListener("resize", onClose);
    return () => {
      window.removeEventListener("pointerdown", down, true);
      window.removeEventListener("keydown", key, true);
      window.removeEventListener("blur", onClose);
      window.removeEventListener("resize", onClose);
    };
  }, [x, y, onClose]);

  // Focus the first enabled item on open only (items may change later).
  useEffect(() => {
    ref.current?.querySelector<HTMLElement>("button:not(:disabled)")?.focus();
  }, []);

  const onKeyDown = (e: ReactKeyboardEvent<HTMLDivElement>) => {
    // Keys pressed in the menu are the menu's (a surrounding lightbox must not react to them).
    e.stopPropagation();
    if (e.key !== "ArrowDown" && e.key !== "ArrowUp" && e.key !== "Home" && e.key !== "End") return;
    const list = Array.from(ref.current?.querySelectorAll<HTMLElement>("button:not(:disabled)") ?? []);
    if (list.length === 0) return;
    e.preventDefault();
    const cur = list.indexOf(document.activeElement as HTMLElement);
    let n: number;
    if (e.key === "Home") n = 0;
    else if (e.key === "End") n = list.length - 1;
    else if (e.key === "ArrowDown") n = cur < 0 ? 0 : (cur + 1) % list.length;
    else n = cur < 0 ? list.length - 1 : (cur - 1 + list.length) % list.length;
    list[n].focus();
  };

  return (
    <div ref={ref} className="context-menu" role="menu" style={{ left: x, top: y }} onKeyDown={onKeyDown}>
      {items.map((it, i) =>
        it === "separator" ? (
          <div key={i} role="separator" className="context-menu-sep" />
        ) : (
          <button
            key={i}
            role="menuitem"
            disabled={it.disabled}
            onClick={() => {
              onClose();
              it.onClick();
            }}
          >
            <span>{it.label}</span>
            {it.hint && <span className="hint">{it.hint}</span>}
          </button>
        ),
      )}
    </div>
  );
}
