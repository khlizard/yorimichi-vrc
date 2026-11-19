import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
  type KeyboardEvent,
  type ReactNode,
  type RefObject,
} from "react";
import type { ChooseOptions } from "../api";

export interface PromptOptions {
  title: string;
  label: string;
  initial?: string;
  okLabel?: string;
  multiline?: boolean;
}

export interface PickOptions<T extends string | number> {
  title: string;
  items: { id: T; label: string; hint?: string }[];
  emptyText?: string;
  /** Extra entry shown last (e.g. "create new"); resolves with its id. */
  extra?: { id: string; label: string };
}

interface DialogApi {
  showError(message: string): void;
  /** Resolves with the clicked button id, or cancelId ?? null on Escape. */
  choose(opts: ChooseOptions): Promise<string | null>;
  /** Text input. Resolves with the trimmed text; null when cancelled or empty. */
  prompt(opts: PromptOptions): Promise<string | null>;
  /** List chooser. Resolves with the chosen item id (or the extra id); null when cancelled. */
  pick<T extends string | number>(opts: PickOptions<T>): Promise<T | string | null>;
}

const Ctx = createContext<DialogApi>({
  showError: () => {},
  choose: () => Promise.resolve(null),
  prompt: () => Promise.resolve(null),
  pick: () => Promise.resolve(null),
});

export function useDialog(): DialogApi {
  return useContext(Ctx);
}

interface Choice {
  opts: ChooseOptions;
  resolve: (id: string | null) => void;
}

type Input =
  | { kind: "prompt"; opts: PromptOptions; resolve: (v: string | null) => void }
  | { kind: "pick"; opts: PickOptions<string | number>; resolve: (v: string | number | null) => void };

/** Keep Tab inside the dialog. */
function trapTab(e: KeyboardEvent<HTMLElement>) {
  if (e.key !== "Tab") return;
  const f = Array.from(e.currentTarget.querySelectorAll<HTMLElement>("button:not(:disabled), input, textarea, [role=listbox]"));
  if (f.length === 0) return;
  const first = f[0];
  const last = f[f.length - 1];
  if (e.shiftKey && document.activeElement === first) {
    e.preventDefault();
    last.focus();
  } else if (!e.shiftKey && document.activeElement === last) {
    e.preventDefault();
    first.focus();
  }
}

/** Focus `ref` on mount; give focus back to the previously focused element on unmount. */
function useModalFocus(ref: RefObject<HTMLElement | null>) {
  useEffect(() => {
    const prev = document.activeElement;
    const el = ref.current;
    el?.focus();
    if (el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement) el.select();
    return () => {
      if (prev instanceof HTMLElement && prev.isConnected) prev.focus();
    };
  }, [ref]);
}

function PromptDialog({ opts, done }: { opts: PromptOptions; done: (v: string | null) => void }) {
  const [text, setText] = useState(opts.initial ?? "");
  const ref = useRef<HTMLInputElement & HTMLTextAreaElement>(null);
  useModalFocus(ref);
  const ok = () => {
    const t = text.trim();
    done(t === "" ? null : t);
  };
  return (
    <div className="modal-backdrop">
      <div
        className="modal"
        role="dialog"
        aria-modal="true"
        aria-labelledby="prompt-title"
        onKeyDown={(e) => {
          trapTab(e);
          if (e.key === "Escape") {
            e.preventDefault();
            done(null);
          }
        }}
      >
        <div className="modal-title" id="prompt-title">
          {opts.title}
        </div>
        <div className="modal-body prompt-body">
          <label htmlFor="prompt-input">{opts.label}</label>
          {opts.multiline ? (
            <textarea
              id="prompt-input"
              ref={ref}
              rows={5}
              value={text}
              onChange={(e) => setText(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter" && e.ctrlKey && !e.nativeEvent.isComposing) {
                  e.preventDefault();
                  ok();
                }
              }}
            />
          ) : (
            <input
              id="prompt-input"
              ref={ref}
              type="text"
              value={text}
              onChange={(e) => setText(e.target.value)}
              onKeyDown={(e) => {
                // The Enter that commits an IME composition must not confirm the dialog.
                if (e.key === "Enter" && !e.nativeEvent.isComposing) {
                  e.preventDefault();
                  ok();
                }
              }}
            />
          )}
        </div>
        <div className="modal-footer">
          <button onClick={ok}>{opts.okLabel ?? "OK"}</button>
          <button onClick={() => done(null)}>キャンセル</button>
        </div>
      </div>
    </div>
  );
}

function PickDialog({ opts, done }: { opts: PickOptions<string | number>; done: (v: string | number | null) => void }) {
  const entries: { id: string | number; label: string; hint?: string; extra: boolean }[] = [
    ...opts.items.map((i) => ({ ...i, extra: false })),
    ...(opts.extra ? [{ ...opts.extra, extra: true }] : []),
  ];
  const [active, setActive] = useState(0);
  const ref = useRef<HTMLDivElement>(null);
  useModalFocus(ref);
  useEffect(() => {
    ref.current?.querySelector(`[data-idx="${active}"]`)?.scrollIntoView({ block: "nearest" });
  }, [active]);
  const move = (i: number) => setActive(Math.max(0, Math.min(entries.length - 1, i)));
  return (
    <div className="modal-backdrop">
      <div
        className="modal"
        role="dialog"
        aria-modal="true"
        aria-labelledby="pick-title"
        onKeyDown={(e) => {
          trapTab(e);
          if (e.key === "Escape") {
            e.preventDefault();
            done(null);
          }
        }}
      >
        <div className="modal-title" id="pick-title">
          {opts.title}
        </div>
        <div className="modal-body pick-body">
          {opts.items.length === 0 && opts.emptyText && <div className="muted pick-empty">{opts.emptyText}</div>}
          {entries.length > 0 && (
            <div
              ref={ref}
              className="pick-list"
              role="listbox"
              tabIndex={0}
              aria-labelledby="pick-title"
              aria-activedescendant={`pick-opt-${active}`}
              onKeyDown={(e) => {
                if (e.key === "ArrowDown") move(active + 1);
                else if (e.key === "ArrowUp") move(active - 1);
                else if (e.key === "Home") move(0);
                else if (e.key === "End") move(entries.length - 1);
                else if (e.key === "Enter") done(entries[active].id);
                else return;
                e.preventDefault();
              }}
            >
              {entries.map((it, i) => (
                <div
                  key={`${it.extra ? "x" : "i"}${it.id}`}
                  id={`pick-opt-${i}`}
                  data-idx={i}
                  role="option"
                  aria-selected={i === active}
                  className={`pick-item${i === active ? " active" : ""}${it.extra ? " extra" : ""}`}
                  onClick={() => done(it.id)}
                  onMouseMove={() => i !== active && setActive(i)}
                >
                  <span className="ellipsis">{it.label}</span>
                  {it.hint && <span className="hint">{it.hint}</span>}
                </div>
              ))}
            </div>
          )}
        </div>
        <div className="modal-footer">
          <button onClick={() => done(null)}>キャンセル</button>
        </div>
      </div>
    </div>
  );
}

export function DialogProvider({ children }: { children: ReactNode }) {
  const [messages, setMessages] = useState<string[]>([]);
  const [choices, setChoices] = useState<Choice[]>([]);
  const [inputs, setInputs] = useState<Input[]>([]);
  const showError = useCallback((m: string) => setMessages((p) => [...p, m]), []);
  const choose = useCallback(
    (opts: ChooseOptions) => new Promise<string | null>((resolve) => setChoices((p) => [...p, { opts, resolve }])),
    [],
  );
  const prompt = useCallback(
    (opts: PromptOptions) =>
      new Promise<string | null>((resolve) => setInputs((p) => [...p, { kind: "prompt", opts, resolve }])),
    [],
  );
  const pick = useCallback(
    <T extends string | number>(opts: PickOptions<T>) =>
      new Promise<T | string | null>((resolve) =>
        setInputs((p) => [
          ...p,
          {
            kind: "pick",
            opts: opts as unknown as PickOptions<string | number>,
            resolve: resolve as (v: string | number | null) => void,
          },
        ]),
      ),
    [],
  );
  const api = useMemo(() => ({ showError, choose, prompt, pick }), [showError, choose, prompt, pick]);
  const current = messages[0];
  const choice = choices[0];
  const input = inputs[0];
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
  const finishInput = (v: string | number | null) => {
    (input?.resolve as ((v: string | number | null) => void) | undefined)?.(v);
    setInputs((p) => p.slice(1));
  };

  return (
    <Ctx.Provider value={api}>
      {children}
      {!open && input?.kind === "prompt" && <PromptDialog key={inputs.length} opts={input.opts} done={finishInput} />}
      {!open && input?.kind === "pick" && <PickDialog key={inputs.length} opts={input.opts} done={finishInput} />}
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
