import { useEffect, useRef, type KeyboardEvent, type ReactNode, type RefObject } from "react";

/**
 * Keep Tab inside the element the key event is attached to. `selector` picks what counts as
 * focusable; Shift+Tab from the first control (or from the element itself) wraps to the last one.
 */
export function trapTab(e: KeyboardEvent<Element>, selector: string) {
  if (e.key !== "Tab") return;
  const root = e.currentTarget;
  const list = Array.from(root.querySelectorAll<HTMLElement>(selector));
  if (list.length === 0) {
    e.preventDefault();
    return;
  }
  const first = list[0];
  const last = list[list.length - 1];
  const cur = document.activeElement;
  if (e.shiftKey && (cur === first || cur === root)) {
    e.preventDefault();
    last.focus();
  } else if (!e.shiftKey && cur === last) {
    e.preventDefault();
    first.focus();
  }
}

export interface ModalProps {
  /** Extra classes after "modal" (e.g. "md-modal"). */
  className?: string;
  role?: "dialog" | "alertdialog";
  labelledBy: string;
  describedBy?: string;
  /** Called on Escape (the key is consumed). Omit to leave Escape alone. */
  onEscape?: () => void;
  /** Runs after Tab trapping and Escape handling. */
  onKeyDown?: (e: KeyboardEvent<HTMLDivElement>) => void;
  /** Selector of the controls Tab cycles through. Omit for no trapping. */
  tabSelector?: string;
  /** Do not let key events reach ancestors (the dialog may sit inside a lightbox that also listens). */
  stopKeys?: boolean;
  /** Focus the dialog element itself on open (adds tabIndex -1). Otherwise `focusRef` is focused. */
  focusRoot?: boolean;
  /** Element to focus on open when `focusRoot` is off (inputs are also selected). */
  focusRef?: RefObject<HTMLElement | null>;
  /** Changing this re-focuses (a queued dialog replaced the shown one without unmounting). */
  focusKey?: string;
  /** Lets the owner reach the dialog element (default: internal). */
  rootRef?: RefObject<HTMLDivElement | null>;
  children: ReactNode;
}

/**
 * Backdrop + dialog frame. Moves focus in when it opens (or `focusKey` changes) and gives it back to
 * the previously focused element when it closes.
 */
export function Modal(p: ModalProps) {
  const own = useRef<HTMLDivElement>(null);
  const root = p.rootRef ?? own;
  const { focusRoot, focusRef, focusKey } = p;

  useEffect(() => {
    const prev = document.activeElement;
    const el = focusRoot ? root.current : focusRef?.current;
    el?.focus();
    if (el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement) el.select();
    return () => {
      if (prev instanceof HTMLElement && prev.isConnected) prev.focus();
    };
  }, [root, focusRoot, focusRef, focusKey]);

  return (
    <div className="modal-backdrop">
      <div
        ref={root}
        tabIndex={focusRoot ? -1 : undefined}
        className={p.className ? `modal ${p.className}` : "modal"}
        role={p.role ?? "dialog"}
        aria-modal="true"
        aria-labelledby={p.labelledBy}
        aria-describedby={p.describedBy}
        onKeyDown={(e) => {
          if (p.stopKeys) e.stopPropagation();
          if (p.tabSelector) trapTab(e, p.tabSelector);
          if (p.onEscape && e.key === "Escape") {
            e.preventDefault();
            p.onEscape();
          }
          p.onKeyDown?.(e);
        }}
      >
        {p.children}
      </div>
    </div>
  );
}
