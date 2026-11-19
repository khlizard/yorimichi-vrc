import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState, type ReactNode } from "react";

interface DialogApi {
  showError(message: string): void;
}

const Ctx = createContext<DialogApi>({ showError: () => {} });

export function useDialog(): DialogApi {
  return useContext(Ctx);
}

export function DialogProvider({ children }: { children: ReactNode }) {
  const [messages, setMessages] = useState<string[]>([]);
  const showError = useCallback((m: string) => setMessages((p) => [...p, m]), []);
  const api = useMemo(() => ({ showError }), [showError]);
  const current = messages[0];
  const open = current !== undefined;
  const okRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    if (!open) return;
    const prev = document.activeElement;
    okRef.current?.focus();
    return () => {
      if (prev instanceof HTMLElement) prev.focus();
    };
  }, [open]);

  const close = () => setMessages((p) => p.slice(1));

  return (
    <Ctx.Provider value={api}>
      {children}
      {open && (
        <div className="modal-backdrop">
          <div
            className="modal"
            role="alertdialog"
            aria-modal="true"
            aria-labelledby="modal-title"
            aria-describedby="modal-body"
            onKeyDown={(e) => {
              if (e.key === "Escape" || e.key === "Enter") {
                e.preventDefault();
                close();
              }
            }}
          >
            <div className="modal-title" id="modal-title">
              VrcLogger
            </div>
            <div className="modal-body" id="modal-body">
              {current}
            </div>
            <div className="modal-footer">
              <button ref={okRef} onClick={close}>
                OK
              </button>
            </div>
          </div>
        </div>
      )}
    </Ctx.Provider>
  );
}
