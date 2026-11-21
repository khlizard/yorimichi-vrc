import { useEffect, useRef, useState, type KeyboardEvent } from "react";
import { useDialog } from "../components/Dialog";
import { errText } from "../components/util";
import {
  metadataCancel,
  metadataPlan,
  metadataRun,
  onMetadataProgress,
  type MetaMode,
  type MetaPlan,
  type MetaProgress,
  type MetaRunSummary,
  type MetaTarget,
  type SkipReason,
} from "../metadataApi";
import type { MetadataOpenOptions } from "./MetadataProvider";
import "./metadata.css";

const nf = (n: number) => n.toLocaleString("ja-JP");

const SKIP_LABELS: Record<SkipReason, string> = {
  hasMetadata: "すでにメタデータがある",
  noLog: "その時刻のログがない",
  estimatedTime: "撮影時刻が推定（ファイル名に日時がない）",
  onlineOnly: "オンライン専用（ダウンロードされていない）",
  missing: "ファイルが見つからない",
  unsupportedFormat: "対象外の形式（PNG・JPEG 以外）",
  readOnly: "書き込み禁止",
  unreadable: "読み取れない",
};
const skipLabel = (r: SkipReason, mode: MetaMode) =>
  r === "hasMetadata" && mode === "remove" ? "このアプリが書いたメタデータがない" : SKIP_LABELS[r];

const WRITE_NOTES = [
  "写真ファイルを書き換えます（画像そのものは変わりません）。",
  "Dropbox などに置いている場合、書き換えたファイルが再アップロードされます。",
  "すでにメタデータがある写真には何もしません。",
  "書き込んだ内容は、あとから取り除いて元のファイルに戻せます。",
];
const REMOVE_NOTES = ["このアプリが書き込んだメタデータだけを取り除きます。VRChat などが書いたものには触れません。"];

type Step = "target" | "planning" | "plan" | "running" | "result";

/** Keep Tab inside the dialog. */
function trapTab(e: KeyboardEvent<HTMLElement>) {
  if (e.key !== "Tab") return;
  const f = Array.from(e.currentTarget.querySelectorAll<HTMLElement>("button:not(:disabled), input:not(:disabled)"));
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

export function MetadataDialog({ opts, onClose }: { opts: MetadataOpenOptions; onClose: () => void }) {
  const { showError } = useDialog();
  const fixed = opts.target ?? null;
  const [step, setStep] = useState<Step>(fixed ? "planning" : "target");
  const [mode, setMode] = useState<MetaMode>("write");
  const [from, setFrom] = useState("");
  const [to, setTo] = useState("");
  const [target, setTarget] = useState<MetaTarget | null>(fixed);
  const [plan, setPlan] = useState<MetaPlan | null>(null);
  const [progress, setProgress] = useState<MetaProgress | null>(null);
  const [summary, setSummary] = useState<{ s: MetaRunSummary; mode: MetaMode } | null>(null);
  const [cancelling, setCancelling] = useState(false);
  const ref = useRef<HTMLDivElement>(null);
  const gen = useRef(0);
  const running = step === "running";

  // Focus the dialog on mount; restore focus on unmount.
  useEffect(() => {
    const prev = document.activeElement;
    ref.current?.focus();
    return () => {
      gen.current++;
      if (prev instanceof HTMLElement && prev.isConnected) prev.focus();
    };
  }, []);

  // Keep focus inside the dialog when a step replaces the focused control.
  useEffect(() => {
    const el = ref.current;
    if (el && !el.contains(document.activeElement)) el.focus();
  }, [step]);

  useEffect(() => {
    if (!running) return;
    const un = onMetadataProgress((p) => setProgress(p));
    return () => {
      un.then((f) => f()).catch(() => {});
    };
  }, [running]);

  /** Plan without changing any file. On failure show the error and go back to the previous step. */
  const doPlan = async (t: MetaTarget, m: MetaMode, back: { step: Step; mode: MetaMode; plan: MetaPlan | null } | null) => {
    const g = ++gen.current;
    const prevTarget = target;
    setTarget(t);
    setPlan(null);
    setStep("planning");
    try {
      const p = await metadataPlan(t, m);
      if (g !== gen.current) return;
      setPlan(p);
      setStep("plan");
    } catch (e) {
      if (g !== gen.current) return;
      showError(errText(e));
      if (!back) {
        onClose();
        return;
      }
      setTarget(prevTarget);
      setMode(back.mode);
      setPlan(back.plan);
      setStep(back.step);
    }
  };

  // Opened with a target: plan immediately.
  useEffect(() => {
    if (fixed) void doPlan(fixed, "write", null);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const rangeOk = from !== "" && to !== "" && from <= to;

  const changeMode = (m: MetaMode) => {
    if (m === mode) return;
    setMode(m);
    if (target && (step === "plan" || step === "planning")) void doPlan(target, m, step === "plan" ? { step: "plan", mode, plan } : fixed ? null : { step: "target", mode, plan: null });
  };

  const run = async () => {
    if (!target || !plan || plan.eligible === 0) return;
    setProgress({ done: 0, total: plan.eligible, fileName: "" });
    setCancelling(false);
    setStep("running");
    try {
      const s = await metadataRun(target, mode);
      setSummary({ s, mode });
      setStep("result");
    } catch (e) {
      showError(errText(e));
      setStep("plan");
    }
  };

  const cancelRun = () => {
    setCancelling(true);
    metadataCancel().catch((e) => showError(errText(e)));
  };

  const close = () => {
    if (!running) onClose();
  };

  const radios = (
    <fieldset className="md-modes">
      <legend className="md-legend">操作</legend>
      <label>
        <input type="radio" name="md-mode" checked={mode === "write"} onChange={() => changeMode("write")} />
        書き込む
      </label>
      <label>
        <input type="radio" name="md-mode" checked={mode === "remove"} onChange={() => changeMode("remove")} />
        書き込んだものを取り除く
      </label>
    </fieldset>
  );

  const ex = plan?.example;

  return (
    <div className="modal-backdrop">
      <div
        ref={ref}
        tabIndex={-1}
        className="modal md-modal"
        role="dialog"
        aria-modal="true"
        aria-labelledby="md-title"
        onKeyDown={(e) => {
          trapTab(e);
          if (e.key === "Escape") {
            e.preventDefault();
            close();
          }
        }}
      >
        <div className="modal-title" id="md-title">
          写真のメタデータ
        </div>
        <div className="md-body">
          {step === "target" && (
            <>
              <div className="md-range">
                <label htmlFor="md-from">期間</label>
                <input id="md-from" type="date" value={from} onChange={(e) => setFrom(e.target.value)} />
                <span>〜</span>
                <input id="md-to" type="date" aria-label="期間の終了日" value={to} onChange={(e) => setTo(e.target.value)} />
              </div>
              {from !== "" && to !== "" && from > to && (
                <div className="md-warn" role="alert">
                  開始日は終了日以前にしてください。
                </div>
              )}
              {radios}
            </>
          )}
          {(step === "planning" || step === "plan") && (
            <>
              {fixed && opts.label && <div className="md-label">{opts.label}</div>}
              {radios}
            </>
          )}
          {step === "planning" && (
            <div className="md-muted" role="status">
              対象を確認中…
            </div>
          )}
          {step === "plan" && plan && (
            <>
              <div className="md-count">
                {mode === "write" ? "書き込む対象" : "取り除く対象"}: <span className="md-num">{nf(plan.eligible)}</span> 枚 / 全{" "}
                <span className="md-num">{nf(plan.total)}</span> 枚
              </div>
              {plan.skipped.some((s) => s.count > 0) && (
                <ul className="md-list">
                  {plan.skipped
                    .filter((s) => s.count > 0)
                    .map((s) => (
                      <li key={s.reason}>
                        {skipLabel(s.reason, mode)}: <span className="md-num">{nf(s.count)}</span> 枚
                      </li>
                    ))}
                </ul>
              )}
              {mode === "write" && ex && (
                <>
                  <div className="md-sub">書き込む内容の例</div>
                  <dl className="md-dl">
                    <dt>ファイル</dt>
                    <dd className="ellipsis" title={ex.fileName}>
                      {ex.fileName}
                    </dd>
                    <dt>撮影日時</dt>
                    <dd>{ex.takenAt}</dd>
                    <dt>ワールド</dt>
                    <dd className="ellipsis" title={ex.worldName}>
                      {ex.worldName}
                    </dd>
                    <dt>撮影者</dt>
                    <dd>{ex.author ?? "（なし）"}</dd>
                  </dl>
                </>
              )}
              <ul className="md-list md-notes">
                {(mode === "write" ? WRITE_NOTES : REMOVE_NOTES).map((n) => (
                  <li key={n}>{n}</li>
                ))}
              </ul>
            </>
          )}
          {running && progress && (
            <>
              <div className="md-count">
                <span className="md-num">
                  {nf(progress.done)} / {nf(progress.total)}
                </span>
              </div>
              <div className="ellipsis md-muted" title={progress.fileName}>
                {progress.fileName || " "}
              </div>
              <progress className="md-progress" max={progress.total} value={progress.done} />
            </>
          )}
          {step === "result" && summary && (
            <>
              <div className="md-count" role="status">
                <span className="md-num">{nf(summary.s.done)}</span> 枚
                {summary.mode === "write" ? "に書き込みました。" : "から取り除きました。"}
              </div>
              {(summary.s.skipped > 0 || summary.s.failed > 0) && (
                <div className="md-muted">
                  {[
                    summary.s.skipped > 0 ? `対象外 ${nf(summary.s.skipped)} 枚` : "",
                    summary.s.failed > 0 ? `失敗 ${nf(summary.s.failed)} 枚` : "",
                  ]
                    .filter(Boolean)
                    .join(" / ")}
                </div>
              )}
              {summary.s.cancelled && <div className="md-muted">途中で中止しました。</div>}
              {summary.s.failures.length > 0 && (
                <ul className="md-failures" tabIndex={0} aria-label="失敗したファイル">
                  {summary.s.failures.map((f, i) => (
                    <li key={i}>
                      {f.fileName} — {f.message}
                    </li>
                  ))}
                </ul>
              )}
            </>
          )}
        </div>
        <div className="modal-footer">
          {step === "target" && (
            <>
              <button
                type="button"
                className="button-accent"
                disabled={!rangeOk}
                onClick={() => void doPlan({ kind: "range", from, to }, mode, { step: "target", mode, plan: null })}
              >
                対象を確認
              </button>
              <button type="button" onClick={close}>
                キャンセル
              </button>
            </>
          )}
          {(step === "planning" || step === "plan") && (
            <>
              <button
                type="button"
                className="button-accent"
                disabled={step !== "plan" || !plan || plan.eligible === 0}
                onClick={() => void run()}
              >
                {plan ? `${nf(plan.eligible)} 枚${mode === "write" ? "に書き込む" : "から取り除く"}` : mode === "write" ? "書き込む" : "取り除く"}
              </button>
              <button type="button" onClick={close}>
                キャンセル
              </button>
            </>
          )}
          {running && (
            <button type="button" disabled={cancelling} onClick={cancelRun}>
              {cancelling ? "中止しています…" : "中止"}
            </button>
          )}
          {step === "result" && (
            <button type="button" className="button-accent" onClick={onClose}>
              閉じる
            </button>
          )}
        </div>
      </div>
    </div>
  );
}
