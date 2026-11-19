import { useEffect, useRef } from "react";

export interface MenuItem {
  label: string;
  hint?: string;
  onClick: () => void;
}

export function ContextMenu({ x, y, items, onClose }: { x: number; y: number; items: MenuItem[]; onClose: () => void }) {
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const el = ref.current;
    if (el) {
      const r = el.getBoundingClientRect();
      el.style.left = `${Math.max(0, Math.min(x, window.innerWidth - r.width - 2))}px`;
      el.style.top = `${Math.max(0, Math.min(y, window.innerHeight - r.height - 2))}px`;
      el.querySelector<HTMLElement>("button")?.focus();
    }
    const down = (e: PointerEvent) => {
      if (!ref.current?.contains(e.target as Node)) onClose();
    };
    const key = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
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

  return (
    <div ref={ref} className="context-menu" role="menu" style={{ left: x, top: y }}>
      {items.map((it) => (
        <button
          key={it.label}
          role="menuitem"
          onClick={() => {
            onClose();
            it.onClick();
          }}
        >
          <span>{it.label}</span>
          {it.hint && <span className="hint">{it.hint}</span>}
        </button>
      ))}
    </div>
  );
}
