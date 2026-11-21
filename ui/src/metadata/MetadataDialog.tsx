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
  SKIP_LABELS,
  type SkipReason,
} from "../metadataApi";
import type { MetadataOpenOptions } from "./MetadataProvider";
import "./metadata.css";

const nf = (n: number) => n.toLocaleString("ja-JP");
const size = (bytes: number) =>
  bytes >= 1e9 ? `${(bytes / 1e9).toFixed(1)} GB` : `${Math.max(1, Math.round(bytes / 1e6)).toLocaleString("ja-JP")} MB`;

const skipLabel = (r: SkipReason, mode: MetaMode) =>
  r === "hasMetadata" && mode === "remove" ? "このアプリが書いたメタデータがない" : SKIP_LABELS[r];

const ONLINE_ONLY_HINT =
  "、同期アプリのメニューでオンライン専用に戻してください（Dropbox は「オンライン専用にする」、OneDrive は「空き領域を増やす」）。";

const WRITE_NOTES = [
  "写真ファイルを書き換えます（画像そのものは変わりません）。",
  "同期アプリ（Dropbox、OneDrive など）のフォルダに置いている場合、書き換えたファイルが再アップロードされます。",
  "すでにメタデータがある写真には何もしません。",
  "書き込んだ内容は、あとから取り除いて元のファイルに戻せます。",
];
const REMOVE_NOTES = ["このアプリが書き込んだメタデータだけを取り除きます。VRChat などが書いたものには触れません。"];

type Step = "planning" | "plan" | "running" | "result";

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
  const target = opts.target;
  const [step, setStep] = useState<Step>("planning");
  const [mode, setMode] = useState<MetaMode>(opts.mode ?? "write");
  const [plan, setPlan] = useState<MetaPlan | null>(null);
  const [progress, setProgress] = useState<MetaProgress | null>(null);
  const [summary, setSummary] = useState<{ s: MetaRunSummary; mode: MetaMode } | null>(null);
  const [cancelling, setCancelling] = useState(false);
  const [allowDownload, setAllowDownload] = useState(opts.allowDownload ?? false);
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
      setMode(back.mode);
      setPlan(back.plan);
      setStep(back.step);
    }
  };

  // Opened with a target: plan immediately.
  useEffect(() => {
    void doPlan(target, mode, null);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const changeMode = (m: MetaMode) => {
    if (m === mode) return;
    setMode(m);
    if (step === "plan" || step === "planning") void doPlan(target, m, step === "plan" ? { step: "plan", mode, plan } : null);
  };

  const withDownload = mode === "write" && allowDownload && (plan?.download ?? 0) > 0;
  const runCount = plan ? plan.eligible + (withDownload ? plan.download : 0) : 0;

  const run = async () => {
    if (!plan || runCount === 0) return;
    setProgress({ done: 0, total: runCount, fileName: "", downloading: false });
    setCancelling(false);
    setStep("running");
    try {
      const s = await metadataRun(target, mode, withDownload);
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
          {(step === "planning" || step === "plan") && (
            <>
              {opts.label && <div className="md-label">{opts.label}</div>}
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
              {mode === "write" && plan.download > 0 && (
                <div className="md-download">
                  <label>
                    <input type="checkbox" checked={allowDownload} onChange={(e) => setAllowDownload(e.target.checked)} />
                    オンライン専用の写真もダウンロードして確認する（<span className="md-num">{nf(plan.download)}</span> 枚、
                    {size(plan.downloadBytes)}）
                  </label>
                  <div className="md-muted">
                    1 枚ずつダウンロードし、メタデータが無ければ書き込みます。ダウンロードしたファイルはこの PC に残ります。不要になったら
                    {ONLINE_ONLY_HINT}
                  </div>
                </div>
              )}
              {mode === "write" && plan.online > 0 && plan.download === 0 && (
                <div className="md-muted">
                  オンライン専用の写真 {nf(plan.online)} 枚に、ダウンロードすれば書き込めるものはありません（上の理由で対象外）。
                </div>
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
                {progress.fileName ? `${progress.downloading ? "ダウンロードして確認中: " : ""}${progress.fileName}` : " "}
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
              {summary.s.downloaded > 0 && (
                <div className="md-muted">
                  オンライン専用の写真を {nf(summary.s.downloaded)} 枚（{size(summary.s.downloadedBytes)}）ダウンロードして確認しました。
                  これらはこの PC に残っています。不要なら{ONLINE_ONLY_HINT}
                </div>
              )}
              {summary.s.lowDisk && (
                <div className="md-warn" role="alert">
                  ディスクの空きが少なくなったため、ダウンロードを途中で止めました。
                </div>
              )}
              {summary.s.downloadStopped && (
                <div className="md-warn" role="alert">
                  ダウンロードの失敗が続いたため、途中で止めました。同期アプリ（Dropbox、OneDrive など）が動いているか確認してください。
                </div>
              )}
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
          {(step === "planning" || step === "plan") && (
            <>
              <button
                type="button"
                className="button-accent"
                disabled={step !== "plan" || !plan || runCount === 0}
                onClick={() => void run()}
              >
                {plan
                  ? `${withDownload ? "最大 " : ""}${nf(runCount)} 枚${mode === "write" ? "に書き込む" : "から取り除く"}`
                  : mode === "write"
                    ? "書き込む"
                    : "取り除く"}
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
