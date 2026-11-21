import { forwardRef, useEffect, useImperativeHandle, useMemo, useRef, useState } from "react";
import { useDialog } from "../../components/Dialog";
import { useLatest } from "../../components/hooks";
import { errText, type TabHandle } from "../../components/util";
import { activity, stats, years as fetchYears, type DayActivity, type StatsSummary } from "../../libraryApi";
import { useNav } from "../../nav";
import { fmtDur, nf, pad } from "../../format";
import "./stats.css";

interface Props {
  active: boolean;
  from: string | null;
  to: string | null;
}

const ymd = (d: Date) => `${d.getUTCFullYear()}-${pad(d.getUTCMonth() + 1)}-${pad(d.getUTCDate())}`;
const DAY_MS = 86400000;

function level(seconds: number, photos: number): number {
  const t = seconds <= 0 ? 0 : seconds <= 3600 ? 1 : seconds <= 7200 ? 2 : seconds <= 14400 ? 3 : 4;
  const p = photos <= 0 ? 0 : photos < 5 ? 1 : photos < 20 ? 2 : photos < 50 ? 3 : 4;
  return Math.max(t, p);
}

export const StatsTab = forwardRef<TabHandle, Props>(function StatsTab({ active, from, to }, ref) {
  const { showError } = useDialog();
  const nav = useNav();
  const [yearList, setYearList] = useState<number[]>([]);
  const [sel, setSel] = useState<string>("");
  const [data, setData] = useState<StatsSummary | null>(null);
  const [acts, setActs] = useState<Map<string, DayActivity>>(() => new Map());
  const [focusDay, setFocusDay] = useState<string | null>(null);
  const gen = useRef(0);
  const started = useRef(false);
  const dirty = useRef(false);
  const selRef = useRef("");
  const range = useLatest({ from, to });
  const activeRef = useLatest(active);
  const gridRef = useRef<HTMLDivElement>(null);
  const loadRef = useRef<(refetchYears: boolean) => Promise<void>>(async () => {});

  const hasRange = from != null || to != null;
  const heatYear = !hasRange && sel !== "" && sel !== "all" ? Number(sel) : null;

  loadRef.current = async (refetchYears: boolean) => {
    const g = ++gen.current;
    try {
      if (refetchYears || selRef.current === "") {
        const ys = await fetchYears();
        if (g !== gen.current) return;
        const sorted = [...ys].sort((a, b) => b - a);
        setYearList(sorted);
        if (selRef.current === "" || (selRef.current !== "all" && !sorted.includes(Number(selRef.current)))) {
          selRef.current = sorted.length > 0 ? String(sorted[0]) : "all";
          setSel(selRef.current);
        }
      }
      const { from: f, to: t } = range.current;
      const cur = selRef.current;
      const useRange = f != null || t != null;
      const year = !useRange && cur !== "all" ? Number(cur) : null;
      const [s, a] = await Promise.all([
        useRange ? stats(f, t) : year != null ? stats(`${year}-01-01`, `${year}-12-31`) : stats(null, null),
        year != null ? activity(`${year}-01-01`, `${year}-12-31`) : Promise.resolve([] as DayActivity[]),
      ]);
      if (g !== gen.current) return;
      setData(s);
      setActs(new Map(a.map((x) => [x.day, x])));
    } catch (e) {
      if (g === gen.current) showError(errText(e));
    }
  };

  useEffect(() => {
    if (!active) return;
    if (!started.current) {
      started.current = true;
      void loadRef.current(true);
    } else if (dirty.current) {
      dirty.current = false;
      void loadRef.current(true);
    }
  }, [active]);

  useImperativeHandle(
    ref,
    () => ({
      refresh() {
        if (!started.current) return;
        if (!activeRef.current) dirty.current = true;
        else void loadRef.current(true);
      },
    }),
    [activeRef],
  );

  // Heatmap geometry for the selected year.
  const heat = useMemo(() => {
    if (heatYear == null) return null;
    const start = Date.UTC(heatYear, 0, 1);
    const w0 = new Date(start).getUTCDay();
    const total = (Date.UTC(heatYear + 1, 0, 1) - start) / DAY_MS;
    const cells: { day: string; col: number; row: number }[] = [];
    const months: { col: number; label: string }[] = [];
    for (let i = 0; i < total; i++) {
      const d = new Date(start + i * DAY_MS);
      const col = Math.floor((i + w0) / 7);
      cells.push({ day: ymd(d), col, row: d.getUTCDay() });
      if (d.getUTCDate() === 1) months.push({ col, label: `${d.getUTCMonth() + 1}月` });
    }
    // Days after today are not rendered at all (no cell, not focusable).
    const n = new Date();
    const today = `${n.getFullYear()}-${pad(n.getMonth() + 1)}-${pad(n.getDate())}`;
    const shown = cells.filter((c) => c.day <= today);
    if (shown.length === 0) return { cells: shown, months: [], first: ymd(new Date(start)), last: ymd(new Date(start)) };
    return { cells: shown, months: months.filter((m) => m.col <= shown[shown.length - 1].col), first: shown[0].day, last: shown[shown.length - 1].day };
  }, [heatYear]);

  // Roving tab stop: the newest active day of the year, else Jan 1.
  const tabStop = useMemo(() => {
    if (!heat) return null;
    if (focusDay && focusDay >= heat.first && focusDay <= heat.last) return focusDay;
    let best: string | null = null;
    for (const [d, a] of acts) if ((a.seconds > 0 || a.photoCount > 0) && (best == null || d > best)) best = d;
    return best ?? heat.first;
  }, [heat, focusDay, acts]);

  const moveFocus = (day: string, delta: number) => {
    if (!heat) return;
    const t = new Date(`${day}T00:00:00Z`).getTime() + delta * DAY_MS;
    const n = ymd(new Date(t));
    if (n < heat.first || n > heat.last) return;
    setFocusDay(n);
    requestAnimationFrame(() => gridRef.current?.querySelector<HTMLElement>(`[data-day="${n}"]`)?.focus());
  };

  const onCellKey = (e: React.KeyboardEvent, day: string) => {
    const d = e.key === "ArrowLeft" ? -7 : e.key === "ArrowRight" ? 7 : e.key === "ArrowUp" ? -1 : e.key === "ArrowDown" ? 1 : 0;
    if (d === 0) return;
    e.preventDefault();
    moveFocus(day, d);
  };

  const onYear = (v: string) => {
    selRef.current = v;
    setSel(v);
    setFocusDay(null);
    void loadRef.current(false);
  };

  const empty = data != null && data.visitCount === 0 && data.photoCount === 0;
  const topPlayers = data?.topPlayers.slice(0, 20) ?? [];
  const topWorlds = data?.topWorlds.slice(0, 20) ?? [];
  const maxP = Math.max(1, ...topPlayers.map((x) => x.seconds));
  const maxW = Math.max(1, ...topWorlds.map((x) => x.seconds));

  return (
    <div className="st">
      <div className="st-controls">
        {hasRange ? (
          <span>
            期間: {range.current.from ?? "最初"} 〜 {range.current.to ?? "最後"}
          </span>
        ) : (
          <>
            <label htmlFor="stats-year">対象:</label>
            <select id="stats-year" value={sel} onChange={(e) => onYear(e.target.value)}>
              {yearList.map((y) => (
                <option key={y} value={String(y)}>
                  {y} 年
                </option>
              ))}
              <option value="all">全期間</option>
            </select>
          </>
        )}
      </div>
      {data && (
        <dl className="st-summary">
          <div>
            <dt>プレイ時間</dt>
            <dd>{fmtDur(data.totalSeconds)}</dd>
          </div>
          <div>
            <dt>活動した日数</dt>
            <dd>{nf(data.activeDays)} 日</dd>
          </div>
          <div>
            <dt>セッション数</dt>
            <dd>{nf(data.visitCount)}</dd>
          </div>
          <div>
            <dt>訪れたワールド数</dt>
            <dd>{nf(data.worldCount)}</dd>
          </div>
          <div>
            <dt>会った人数</dt>
            <dd>{nf(data.peopleCount)}</dd>
          </div>
          <div>
            <dt>写真の枚数</dt>
            <dd>{nf(data.photoCount)}</dd>
          </div>
        </dl>
      )}
      {heat && (
        <section aria-label="活動量">
          <h3>日ごとの活動（プレイ時間と写真の枚数）</h3>
          <div className="st-heat-wrap">
            <div className="st-heat" ref={gridRef} role="group" aria-label={`${heatYear} 年の活動量`}>
              {heat.months.map((m) => (
                <span key={m.label} className="lbl" aria-hidden="true" style={{ gridRow: 1, gridColumn: `${m.col + 2} / span 3` }}>
                  {m.label}
                </span>
              ))}
              {[1, 3, 5].map((w) => (
                <span key={w} className="lbl" aria-hidden="true" style={{ gridRow: w + 2, gridColumn: 1 }}>
                  {"日月火水木金土"[w]}
                </span>
              ))}
              {heat.cells.map((c) => {
                const a = acts.get(c.day);
                const sec = a?.seconds ?? 0;
                const pc = a?.photoCount ?? 0;
                const parts = [sec > 0 ? fmtDur(sec) : "", pc > 0 ? `写真 ${nf(pc)} 枚` : ""].filter((s) => s !== "");
                const text = `${c.day}: ${parts.length > 0 ? parts.join("、") : "記録なし"}`;
                return (
                  <button
                    key={c.day}
                    type="button"
                    data-day={c.day}
                    className={`cell-btn l${level(sec, pc)}`}
                    title={text}
                    aria-label={text}
                    tabIndex={c.day === tabStop ? 0 : -1}
                    style={{ gridRow: c.row + 2, gridColumn: c.col + 2 }}
                    onFocus={() => setFocusDay(c.day)}
                    onKeyDown={(e) => onCellKey(e, c.day)}
                    onClick={() => nav.showDay(c.day)}
                  />
                );
              })}
            </div>
          </div>
          <div className="st-legend">
            <span aria-hidden="true" className="st-legend-scale">
              少
              <i className="l0" />
              <i className="l1" />
              <i className="l2" />
              <i className="l3" />
              <i className="l4" />多
            </span>
            <span>濃さは、プレイ時間（1h / 2h / 4h）と写真の枚数（5 / 20 / 50 枚）の多い方で決まります。</span>
          </div>
        </section>
      )}
      {empty && <p className="st-empty">この期間の記録はありません。</p>}
      {data && !empty && (
        <div className="st-cols">
          <section aria-label="よく会う人">
            <h3>よく会う人</h3>
            <ul className="st-rank">
              {topPlayers.map((p) => (
                <li key={p.playerId}>
                  <span className="st-bar" style={{ width: `${(p.seconds / maxP) * 100}%` }} aria-hidden="true" />
                  <button type="button" className="name" onClick={() => nav.showPlayer(p.playerId, p.displayName)}>
                    {p.displayName}
                  </button>
                  <span className="num">{fmtDur(p.seconds)}</span>
                  <span className="num">{nf(p.count)} 回</span>
                </li>
              ))}
            </ul>
          </section>
          <section aria-label="よく行くワールド">
            <h3>よく行くワールド</h3>
            <ul className="st-rank">
              {topWorlds.map((w, i) => (
                <li key={`${w.worldRefId ?? "x"}-${i}`}>
                  <span className="st-bar" style={{ width: `${(w.seconds / maxW) * 100}%` }} aria-hidden="true" />
                  {w.worldRefId != null ? (
                    <button type="button" className="name" onClick={() => nav.showWorld(w.worldRefId!)}>
                      {w.name}
                    </button>
                  ) : (
                    <span className="name">{w.name}</span>
                  )}
                  <span className="num">{fmtDur(w.seconds)}</span>
                  <span className="num">{nf(w.count)} 回</span>
                </li>
              ))}
            </ul>
          </section>
        </div>
      )}
    </div>
  );
});
