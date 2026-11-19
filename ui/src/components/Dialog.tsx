import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import type { ChooseOptions } from "../api";

interface DialogApi {
  showError(message: string): void;
  /** Resolves with the clicked button id, or cancelId ?? null on Escape. */
  choose(opts: ChooseOptions): Promise<string | null>;
}

const Ctx = createContext<DialogApi>({ showError: () => {}, choose: () => Promise.resolve(null) });

export function useDialog(): DialogApi {
  return useContext(Ctx);
}

interface Choice {
  opts: ChooseOptions;
  resolve: (id: string | null) => void;
}

export function DialogProvider({ children }: { children: ReactNode }) {
  const [messages, setMessages] = useState<string[]>([]);
  const [choices, setChoices] = useState<Choice[]>([]);
  const showError = useCallback((m: string) => setMessages((p) => [...p, m]), []);
  const choose = useCallback(
    (opts: ChooseOptions) => new Promise<string | null>((resolve) => setChoices((p) => [...p, { opts, resolve }])),
    [],
  );
  const api = useMemo(() => ({ showError, choose }), [showError, choose]);
  const current = messages[0];
  const choice = choices[0];
  const open = current !== undefined || choice !== undefined;
  const okRef = useRef<HTMLButtonElement>(null);
  const primaryRef = useRef<HTMLButtonElement>(null);

  // Re-focus whenever the shown dialog changes (queued dialogs replace each other).
  const shownKey = choice ? `c${choices.length}` : `m${messages.length}`;
  useEffect(() => {
    if (!open) return;
    const prev = document.activeElement;
    (primaryRef.current ?? okRef.current)?.focus();
    return () => {
      if (prev instanceof HTMLElement) prev.focus();
    };
  }, [open, shownKey]);

  const close = () => setMessages((p) => p.slice(1));
  const answer = (id: string | null) => {
    choice?.resolve(id);
    setChoices((p) => p.slice(1));
  };

  return (
    <Ctx.Provider value={api}>
      {children}
      {choice ? (
        <div className="modal-backdrop">
          <div
            className="modal"
            role="dialog"
            aria-modal="true"
            aria-labelledby="modal-title"
            aria-describedby="modal-body"
            onKeyDown={(e) => {
              if (e.key === "Escape") {
                e.preventDefault();
                answer(choice.opts.cancelId ?? null);
              }
            }}
          >
            <div className="modal-title" id="modal-title">
              {choice.opts.title}
            </div>
            <div className="modal-body" id="modal-body">
              {choice.opts.message}
            </div>
            <div className="modal-footer">
              {choice.opts.buttons.map((b) => (
                <button key={b.id} ref={b.primary ? primaryRef : undefined} onClick={() => answer(b.id)}>
                  {b.label}
                </button>
              ))}
            </div>
          </div>
        </div>
      ) : (
        current !== undefined && (
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
        )
      )}
    </Ctx.Provider>
  );
}
