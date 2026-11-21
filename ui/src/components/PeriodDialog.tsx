import { useRef, useState } from "react";
import { Modal } from "./Modal";

export interface Period {
  /** yyyy-mm-dd, or null for "from the first day" */
  from: string | null;
  /** yyyy-mm-dd, or null for "up to the last day" */
  to: string | null;
}

const TAB = "button:not(:disabled), input";

/** "2026-01-31" -> "2026/01/31"; null -> the open-end word. */
export const periodDate = (d: string | null, open: string) => (d ? d.replaceAll("-", "/") : open);

/**
 * Date range filter, opened from the menu (表示 → 期間で絞り込む…) or the period chip.
 * Either end may stay empty. A reversed range is swapped, not rejected.
 */
export function PeriodDialog({ initial, done }: { initial: Period; done: (p: Period | null) => void }) {
  const [from, setFrom] = useState(initial.from ?? "");
  const [to, setTo] = useState(initial.to ?? "");
  const first = useRef<HTMLInputElement>(null);
  const active = initial.from != null || initial.to != null;

  const apply = () => {
    let f: string | null = from || null;
    let t: string | null = to || null;
    if (f && t && f > t) [f, t] = [t, f];
    done({ from: f, to: t });
  };

  return (
    <Modal
      labelledBy="period-title"
      tabSelector={TAB}
      focusRef={first}
      onEscape={() => done(null)}
      onKeyDown={(e) => {
        if (e.key === "Enter" && e.target instanceof HTMLInputElement) {
          e.preventDefault();
          apply();
        }
      }}
    >
      <div className="modal-title" id="period-title">
        期間で絞り込む
      </div>
      <div className="modal-body period-body">
        <p className="period-note">
          写真、プレイヤー、セッション、統計などを、この期間に絞ります。
          <br />
          空欄の側は制限しません。タイムライン、アルバム、ワールドには効きません。
        </p>
        <div className="period-fields">
          <label htmlFor="period-from">開始日</label>
          <input id="period-from" ref={first} type="date" value={from} onChange={(e) => setFrom(e.target.value)} />
          <label htmlFor="period-to">終了日</label>
          <input id="period-to" type="date" value={to} onChange={(e) => setTo(e.target.value)} />
        </div>
      </div>
      <div className="modal-footer">
        {active && (
          <button className="period-clear" onClick={() => done({ from: null, to: null })}>
            解除
          </button>
        )}
        <button className="button-accent" onClick={apply}>
          絞り込む
        </button>
        <button onClick={() => done(null)}>キャンセル</button>
      </div>
    </Modal>
  );
}
