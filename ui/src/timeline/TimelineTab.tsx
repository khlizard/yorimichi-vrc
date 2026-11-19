import { forwardRef, useEffect, useImperativeHandle, useMemo, useRef, useState } from "react";
import { thumbUrl, type PhotoRow } from "../api";
import { useDialog } from "../components/Dialog";
import { useLatest } from "../components/hooks";
import { errText, type TabHandle } from "../components/util";
import {
  adjacentActiveDay,
  exportDayHtml,
  latestActiveDay,
  onThisDay,
  openWorldPage,
  timelineDay,
  type MemoryYear,
  type TimelineDay,
  type TimelinePlayer,
  type TimelineVisit,
} from "../libraryApi";
import { useNav } from "../nav";
import { Lightbox } from "../photos/Lightbox";
import { dayLabel, logicalDay, photoLabel } from "../photos/photoUtil";
import "../photos/photos.css";
import { fmtDur, nf } from "./format";
import "./timeline.css";

export interface TimelineHandle extends TabHandle {
  showDay(day: string): void;
}

interface Props {
  active: boolean;
  dayStartHour: number;
}

const PEOPLE_CAP = 24;
const PHOTO_CAP = 60;
const MEMORY_THUMBS = 8;

type Stop =
  | { kind: "visit"; key: string; time: string; visit: TimelineVisit }
  | { kind: "loose"; key: string; time: string; photos: PhotoRow[] };

function nowText(): string {
  const d = new Date();
  const p = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}:00`;
}

const aspect = (p: PhotoRow) => (p.width && p.height ? `${p.width} / ${p.height}` : "16 / 9");

function buildStops(data: TimelineDay): Stop[] {
  const visits = [...data.visits].sort((a, b) => a.joinTime.localeCompare(b.joinTime));
  const stops: Stop[] = visits.map((v) => ({ kind: "visit", key: `v${v.visitId}`, time: v.joinTime, visit: v }));
  // Loose photos go into the slot between the visits they fall between.
  const slots = new Map<number, PhotoRow[]>();
  const loose = [...data.loosePhotos].sort((a, b) => a.takenAt.localeCompare(b.takenAt));
  for (const p of loose) {
    let slot = 0;
    while (slot < visits.length && visits[slot].joinTime <= p.takenAt) slot++;
    const l = slots.get(slot);
    if (l) l.push(p);
    else slots.set(slot, [p]);
  }
  for (const [slot, photos] of slots) stops.push({ kind: "loose", key: `l${slot}`, time: photos[0].takenAt, photos });
  stops.sort((a, b) => a.time.localeCompare(b.time) || (a.kind === "visit" ? -1 : 1));
  return stops;
}

function Star() {
  return (
    <svg className="tl-star" width="13" height="13" viewBox="0 0 24 24" role="img" aria-label="お気に入り">
      <title>お気に入り</title>
      <path fill="currentColor" d="M12 2l3.1 6.6 7.2.9-5.3 5 1.4 7.1L12 18l-6.4 3.6 1.4-7.1-5.3-5 7.2-.9z" />
    </svg>
  );
}

function Tile({ p, mini, onOpen }: { p: PhotoRow; mini?: boolean; onOpen: () => void }) {
  return (
    <button
      type="button"
      className={`tl-tile${mini ? " mini" : ""}`}
      data-photo-id={p.id}
      aria-label={photoLabel(p)}
      style={{ aspectRatio: aspect(p) }}
      onClick={onOpen}
    >
      <img loading="lazy" decoding="async" src={thumbUrl(p.id)} alt="" draggable={false} />
    </button>
  );
}

export const TimelineTab = forwardRef<TimelineHandle, Props>(function TimelineTab({ active, dayStartHour }, ref) {
  const { showError, choose } = useDialog();
  const nav = useNav();
  const [day, setDay] = useState<string | null>(null);
  const [data, setData] = useState<TimelineDay | null>(null);
  const [memories, setMemories] = useState<MemoryYear[]>([]);
  const [adj, setAdj] = useState<{ prev: string | null; next: string | null }>({ prev: null, next: null });
  const [lb, setLb] = useState<{ items: PhotoRow[]; index: number } | null>(null);
  const [morePeople, setMorePeople] = useState<Set<string>>(() => new Set());
  const [morePhotos, setMorePhotos] = useState<Set<string>>(() => new Set());
  const [exporting, setExporting] = useState(false);
  const gen = useRef(0);
  const started = useRef(false);
  const dirty = useRef(false);
  const dayRef = useRef<string | null>(null);
  const bodyRef = useRef<HTMLDivElement>(null);
  const activeRef = useLatest(active);
  const dayStartHourRef = useLatest(dayStartHour);
  const loadRef = useRef<(d: string, keepScroll: boolean) => Promise<void>>(async () => {});

  loadRef.current = async (d: string, keepScroll: boolean) => {
    const g = ++gen.current;
    dayRef.current = d;
    setDay(d);
    try {
      const [t, m, prev, next] = await Promise.all([
        timelineDay(d),
        onThisDay(d),
        adjacentActiveDay(d, false),
        adjacentActiveDay(d, true),
      ]);
      if (g !== gen.current) return;
      setData(t);
      setMemories(m);
      setAdj({ prev, next });
      setLb(null);
      if (!keepScroll) {
        setMorePeople(new Set());
        setMorePhotos(new Set());
        bodyRef.current?.scrollTo({ top: 0 });
      }
    } catch (e) {
      if (g === gen.current) showError(errText(e));
    }
  };

  const goTo = (d: string) => void loadRef.current(d, false);

  const showLatest = async () => {
    try {
      const d = await latestActiveDay();
      goTo(d ?? logicalDay(nowText(), dayStartHourRef.current));
    } catch (e) {
      showError(errText(e));
    }
  };


  // First show: load the newest day with activity.
  useEffect(() => {
    if (!active) return;
    if (!started.current) {
      started.current = true;
      void showLatest();
    } else if (dirty.current && dayRef.current) {
      dirty.current = false;
      void loadRef.current(dayRef.current, true);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [active]);

  useImperativeHandle(
    ref,
    () => ({
      refresh() {
        if (!started.current) return;
        if (!activeRef.current) dirty.current = true;
        else if (dayRef.current) void loadRef.current(dayRef.current, true);
      },
      showDay(d: string) {
        started.current = true;
        dirty.current = false;
        void loadRef.current(d, false);
      },
    }),
    [activeRef],
  );

  const stops = useMemo(() => (data ? buildStops(data) : []), [data]);
  const allPhotos = useMemo(() => {
    if (!data) return [];
    const l = [...data.visits.flatMap((v) => v.photos), ...data.loosePhotos];
    return l.sort((a, b) => a.takenAt.localeCompare(b.takenAt));
  }, [data]);

  const openDayPhoto = (id: number) => {
    const index = allPhotos.findIndex((p) => p.id === id);
    if (index >= 0) setLb({ items: allPhotos, index });
  };

  const step = (forward: boolean) => {
    const d = forward ? adj.next : adj.prev;
    if (d) goTo(d);
  };

  const onKeyDown = (e: React.KeyboardEvent) => {
    if (lb) return;
    const t = e.target as HTMLElement;
    if (t.closest("input, textarea, select")) return;
    if (e.key === "[" || (e.altKey && e.key === "ArrowLeft")) {
      e.preventDefault();
      step(false);
    } else if (e.key === "]" || (e.altKey && e.key === "ArrowRight")) {
      e.preventDefault();
      step(true);
    }
  };

  const exportDay = async () => {
    if (!day || exporting) return;
    const id = await choose({
      title: "HTML に書き出す",
      message: "会った人の名前を含めますか？",
      buttons: [
        { id: "include", label: "含める", primary: true },
        { id: "exclude", label: "含めない" },
        { id: "cancel", label: "キャンセル" },
      ],
      cancelId: "cancel",
    });
    if (id !== "include" && id !== "exclude") return;
    setExporting(true);
    try {
      await exportDayHtml(day, id === "include");
    } catch (e) {
      showError(errText(e));
    } finally {
      setExporting(false);
    }
  };

  const toggle = (set: Set<string>, key: string) => {
    const n = new Set(set);
    if (n.has(key)) n.delete(key);
    else n.add(key);
    return n;
  };

  const empty = data != null && data.visits.length === 0 && data.loosePhotos.length === 0;
  const thisYear = day ? Number(day.slice(0, 4)) : 0;

  const renderPeople = (v: TimelineVisit) => {
    const key = String(v.visitId);
    const sorted: TimelinePlayer[] = [...v.players].sort((a, b) => a.joinTime.localeCompare(b.joinTime));
    if (sorted.length === 0) return null;
    const expanded = morePeople.has(key);
    const shown = expanded ? sorted : sorted.slice(0, PEOPLE_CAP);
    return (
      <div className="tl-people" role="group" aria-label="会った人">
        {shown.map((p) => (
          <span className="tl-person" key={`${p.playerId}-${p.joinTime}`}>
            <button type="button" onClick={() => nav.showPlayer(p.playerId, p.displayName)}>
              {p.displayName}
            </button>
            {p.firstMet && <span className="tl-chip">初対面</span>}
            {p.daysSinceLast != null && <span className="tl-chip">{nf(p.daysSinceLast)} 日ぶり</span>}
            {p.hasMemo && (
              <span className="tl-memo-dot" title="メモあり" role="img" aria-label="メモあり" />
            )}
          </span>
        ))}
        {sorted.length > PEOPLE_CAP && (
          <button type="button" className="tl-small" aria-expanded={expanded} onClick={() => setMorePeople((s) => toggle(s, key))}>
            {expanded ? "折りたたむ" : `ほか ${sorted.length - PEOPLE_CAP} 人`}
          </button>
        )}
      </div>
    );
  };

  const renderPhotos = (key: string, photos: PhotoRow[], label: string) => {
    if (photos.length === 0) return null;
    const expanded = morePhotos.has(key);
    const shown = expanded ? photos : photos.slice(0, PHOTO_CAP);
    return (
      <>
        <div className="tl-photos" role="group" aria-label={label}>
          {shown.map((p) => (
            <Tile key={p.id} p={p} onOpen={() => openDayPhoto(p.id)} />
          ))}
        </div>
        {photos.length > PHOTO_CAP && !expanded && (
          <button type="button" className="tl-small tl-more" onClick={() => setMorePhotos((s) => toggle(s, key))}>
            すべて表示 ({nf(photos.length)})
          </button>
        )}
      </>
    );
  };

  return (
    <div className="photo-surface tl" onKeyDown={onKeyDown}>
      <div className="tl-head">
        <button type="button" disabled={!adj.prev} onClick={() => step(false)} title="[ または Alt+←">
          前の日
        </button>
        <button type="button" disabled={!adj.next} onClick={() => step(true)} title="] または Alt+→">
          次の日
        </button>
        <input
          type="date"
          aria-label="日付を指定して移動"
          value={day ?? ""}
          onChange={(e) => e.target.value && goTo(e.target.value)}
        />
        <button type="button" onClick={() => void showLatest()}>
          最新の日
        </button>
        <span className="tl-spacer" />
        <button type="button" disabled={!day || exporting} onClick={() => void exportDay()}>
          この日を HTML に書き出す…
        </button>
      </div>
      {day && (
        <div className="tl-title-row">
          <h2 className="tl-title">{dayLabel(day)}</h2>
          {data && (
            <div className="tl-summary">
              プレイ {fmtDur(data.totalSeconds)} · ワールド {nf(data.visitCount)} · 会った人 {nf(data.peopleCount)} · 写真{" "}
              {nf(data.photoCount)}
            </div>
          )}
        </div>
      )}
      <div className="tl-body" ref={bodyRef}>
        {memories.length > 0 && (
          <section className="tl-memories" aria-label="過去の同じ日">
            {memories.map((m) => (
              <div className="tl-mem" key={m.day}>
                <h3>
                  {nf(thisYear - m.year)} 年前の今日 ({m.day})
                </h3>
                {m.photos.length > 0 && (
                  <div className="tl-mem-thumbs" role="group" aria-label={`${m.day} の写真`}>
                    {m.photos.slice(0, MEMORY_THUMBS).map((p, i) => (
                      <Tile key={p.id} p={p} mini onOpen={() => setLb({ items: m.photos, index: i })} />
                    ))}
                  </div>
                )}
                <div className="tl-mem-line">
                  <span>
                    ワールド {nf(m.visitCount)} · 会った人 {nf(m.peopleCount)} · 写真 {nf(m.photoCount)}
                    {m.worlds.length > 0 && ` — ${m.worlds.join("、")}`}
                  </span>
                  <button type="button" className="tl-small" onClick={() => nav.showDay(m.day)}>
                    この日を開く
                  </button>
                </div>
              </div>
            ))}
          </section>
        )}
        {empty && <p className="tl-empty">この日の記録はありません。</p>}
        {stops.length > 0 && (
          <ol className="tl-rail" aria-label="この日の流れ">
            {stops.map((s) =>
              s.kind === "visit" ? (
                <li className="tl-stop" key={s.key}>
                  <span className="tl-time">{s.visit.joinTime.slice(11, 16)}</span>
                  <span className="tl-mark" aria-hidden="true" />
                  <div className="tl-world">
                    <button type="button" className="tl-link" onClick={() => nav.showVisit(s.visit.visitId)}>
                      {s.visit.worldName}
                    </button>
                    {s.visit.worldFavorite && <Star />}
                    {s.visit.worldId && (
                      <button type="button" className="tl-small" onClick={() => openWorldPage(s.visit.worldId!).catch((e) => showError(errText(e)))}>
                        ワールドのページ
                      </button>
                    )}
                    {s.visit.worldMemo.trim() !== "" && (
                      <button
                        type="button"
                        className="tl-small"
                        title={s.visit.worldMemo}
                        disabled={s.visit.worldRefId == null}
                        onClick={() => s.visit.worldRefId != null && nav.showWorld(s.visit.worldRefId)}
                      >
                        ワールドのメモ
                      </button>
                    )}
                  </div>
                  <div className="tl-meta" title={s.visit.location}>
                    {s.visit.joinTime.slice(11, 16)} – {s.visit.leaveTime.slice(11, 16)} · {s.visit.duration} · インスタンス{" "}
                    {s.visit.instanceId.split("~")[0]}
                  </div>
                  {renderPeople(s.visit)}
                  {renderPhotos(s.key, s.visit.photos, `${s.visit.worldName} の写真`)}
                </li>
              ) : (
                <li className="tl-stop loose" key={s.key}>
                  <span className="tl-time">{s.time.slice(11, 16)}</span>
                  <span className="tl-mark" aria-hidden="true" />
                  <div className="tl-world">
                    <b>ログのない時間の写真</b>
                  </div>
                  {renderPhotos(s.key, s.photos, "ログのない時間の写真")}
                </li>
              ),
            )}
          </ol>
        )}
      </div>
      {lb && (
        <Lightbox
          items={lb.items}
          index={lb.index}
          onIndexChange={(i) => setLb((x) => (x ? { ...x, index: i } : x))}
          onClose={() => setLb(null)}
          onNavigateVisit={(id) => {
            setLb(null);
            nav.showVisit(id);
          }}
          onNavigatePlayer={(id, name) => {
            setLb(null);
            nav.showPlayer(id, name);
          }}
          onFavoriteChanged={(id, fav) => {
            const upd = (l: PhotoRow[]) => l.map((x) => (x.id === id ? { ...x, favorite: fav } : x));
            setLb((x) => (x ? { ...x, items: upd(x.items) } : x));
            setData((d) =>
              d ? { ...d, visits: d.visits.map((v) => ({ ...v, photos: upd(v.photos) })), loosePhotos: upd(d.loosePhotos) } : d,
            );
          }}
        />
      )}
    </div>
  );
});
