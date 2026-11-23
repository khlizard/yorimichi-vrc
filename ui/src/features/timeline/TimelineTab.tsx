import { forwardRef, useEffect, useImperativeHandle, useMemo, useRef, useState } from "react";
import { onPhotosChanged, type PhotoRow } from "../../api";
import { useDialog } from "../../components/Dialog";
import { useLatest } from "../../components/hooks";
import { errText, type TabHandle } from "../../components/util";
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
} from "../../libraryApi";
import { StarIcon } from "../../components/Star";
import { useNav } from "../../nav";
import { Lightbox } from "../photos/Lightbox";
import { usePhotoMenu } from "../photos/usePhotoMenu";
import { Thumb } from "../photos/Thumb";
import { dayLabel, logicalDay, photoLabel } from "../photos/photoUtil";
import "../photos/photos.css";
import { fmtDur, nf, pad } from "../../format";
import { buildStops } from "./stops";
import { useGeneration, useLazyLoad } from "../../components/useLazyLoad";
import { withFavorite } from "../photos/usePhotoSelection";
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

function nowText(): string {
  const d = new Date();
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}:00`;
}

const aspect = (p: PhotoRow) => (p.width && p.height ? `${p.width} / ${p.height}` : "16 / 9");

/** "label n" parts joined by " · ", skipping zeros. */
function counts(parts: [string, number][]): string {
  return parts
    .filter(([, n]) => n > 0)
    .map(([l, n]) => `${l} ${nf(n)}`)
    .join(" · ");
}

function Tile({
  p,
  mini,
  onOpen,
  onContextMenu,
}: {
  p: PhotoRow;
  mini?: boolean;
  onOpen: () => void;
  onContextMenu: (e: React.MouseEvent) => void;
}) {
  return (
    <button
      type="button"
      className={`tl-tile${mini ? " mini" : ""}`}
      data-photo-id={p.id}
      aria-label={photoLabel(p)}
      style={{ aspectRatio: aspect(p) }}
      onClick={onOpen}
      onContextMenu={onContextMenu}
    >
      <Thumb photo={p} />
    </button>
  );
}

export const TimelineTab = forwardRef<TimelineHandle, Props>(function TimelineTab({ active, dayStartHour }, ref) {
  const { showError, choose } = useDialog();
  const nav = useNav();
  const menu = usePhotoMenu();
  const [day, setDay] = useState<string | null>(null);
  const [data, setData] = useState<TimelineDay | null>(null);
  const [memories, setMemories] = useState<MemoryYear[]>([]);
  const [adj, setAdj] = useState<{ prev: string | null; next: string | null }>({ prev: null, next: null });
  const [lb, setLb] = useState<{ items: PhotoRow[]; index: number } | null>(null);
  const [morePeople, setMorePeople] = useState<Set<string>>(() => new Set());
  const [morePhotos, setMorePhotos] = useState<Set<string>>(() => new Set());
  const [exporting, setExporting] = useState(false);
  const gen = useGeneration();
  const dayRef = useRef<string | null>(null);
  const bodyRef = useRef<HTMLDivElement>(null);
  const activeRef = useLatest(active);
  const dayStartHourRef = useLatest(dayStartHour);
  const loadRef = useRef<(d: string, keepScroll: boolean) => Promise<void>>(async () => {});

  loadRef.current = async (d: string, keepScroll: boolean) => {
    const g = gen.next();
    dayRef.current = d;
    setDay(d);
    try {
      const [t, m, prev, next] = await Promise.all([
        timelineDay(d),
        onThisDay(d),
        adjacentActiveDay(d, false),
        adjacentActiveDay(d, true),
      ]);
      if (!gen.isCurrent(g)) return;
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
      if (gen.isCurrent(g)) showError(errText(e));
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
  const lazy = useLazyLoad({
    active,
    first: () => void showLatest(),
    reload: () => {
      if (dayRef.current) void loadRef.current(dayRef.current, true);
    },
    ready: () => !!dayRef.current,
  });

  // Photo files / metadata changed: reload the current day once the tab has been shown.
  useEffect(() => {
    const un = onPhotosChanged(() => {
      if (!lazy.started.current || !dayRef.current) return;
      if (!activeRef.current) lazy.dirty.current = true;
      else void loadRef.current(dayRef.current, true);
    });
    return () => {
      un.then((f) => f()).catch(() => {});
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useImperativeHandle(
    ref,
    () => ({
      refresh: () => lazy.refresh(),
      showDay(d: string) {
        lazy.begin();
        void loadRef.current(d, false);
      },
      getView: () => ({ day: dayRef.current }),
      restoreView(v: unknown) {
        const d = (v as { day?: string | null } | null)?.day;
        if (!d || d === dayRef.current) return;
        lazy.begin();
        void loadRef.current(d, false);
      },
    }),
    // `lazy` only touches refs, so it never goes stale.
    // eslint-disable-next-line react-hooks/exhaustive-deps
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
    if (e.altKey || e.ctrlKey || e.metaKey) return;
    if (e.key === "[") {
      e.preventDefault();
      step(false);
    } else if (e.key === "]") {
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

  const favoriteChanged = (id: number, fav: boolean) => {
    const upd = (l: PhotoRow[]) => withFavorite(l, id, fav);
    setLb((x) => (x ? { ...x, items: upd(x.items) } : x));
    setData((d) =>
      d ? { ...d, visits: d.visits.map((v) => ({ ...v, photos: upd(v.photos) })), loosePhotos: upd(d.loosePhotos) } : d,
    );
    setMemories((ms) => ms.map((m) => ({ ...m, photos: upd(m.photos) })));
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
            <Tile
              key={p.id}
              p={p}
              onOpen={() => openDayPhoto(p.id)}
              onContextMenu={(e) => menu.onContextMenu(e, p, { onOpen: () => openDayPhoto(p.id), onFavoriteChanged: favoriteChanged })}
            />
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
        <button type="button" disabled={!adj.prev} onClick={() => step(false)} title="[ キー">
          前の日
        </button>
        <button type="button" disabled={!adj.next} onClick={() => step(true)} title="] キー">
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
              {[
                data.totalSeconds > 0 ? `プレイ ${fmtDur(data.totalSeconds)}` : "",
                counts([
                  ["ワールド", data.visitCount],
                  ["会った人", data.peopleCount],
                  ["写真", data.photoCount],
                ]),
              ]
                .filter(Boolean)
                .join(" · ")}
            </div>
          )}
        </div>
      )}
      <div className="tl-body" ref={bodyRef}>
        {empty && <p className="tl-empty">この日の記録はありません。</p>}
        {stops.length > 0 && (
          <ol className="tl-rail" aria-label="この日の流れ">
            {stops.map((s) =>
              s.kind === "visit" ? (
                <li className="tl-stop" key={s.key}>
                  {s.visit.carriedOver ? (
                    <span className="tl-time tl-time-carried">前日から</span>
                  ) : (
                    <span className="tl-time">{s.visit.joinTime.slice(11, 16)}</span>
                  )}
                  <span className="tl-mark" aria-hidden="true" />
                  <div className="tl-world">
                    <button type="button" className="tl-link" onClick={() => nav.showVisit(s.visit.visitId)}>
                      {s.visit.worldName}
                    </button>
                    {s.visit.worldFavorite && (
                    <span className="tl-star" role="img" aria-label="お気に入り" title="お気に入り">
                      <StarIcon filled size={13} />
                    </span>
                  )}
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
                    {s.visit.joinTime.slice(11, 16)} – {s.visit.leaveTime.slice(11, 16)} · {s.visit.duration}
                    {s.visit.carriedOver && " · 前日から続く"} · インスタンス{" "}
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
        {memories.length > 0 && (
          <section className="tl-memories" aria-label="過去の同じ日">
            <h3 className="tl-memories-title">過去の同じ日</h3>
            {memories.map((m) => {
              const line = counts([
                ["ワールド", m.visitCount],
                ["会った人", m.peopleCount],
                ["写真", m.photoCount],
              ]);
              const worlds = m.worlds.length > 0 ? m.worlds.join("、") : "";
              return (
                <div className="tl-mem" key={m.day}>
                  <h4>
                    {nf(thisYear - m.year)} 年前の今日 ({m.day})
                  </h4>
                  {m.photos.length > 0 && (
                    <div className="tl-mem-thumbs" role="group" aria-label={`${m.day} の写真`}>
                      {m.photos.slice(0, MEMORY_THUMBS).map((p, i) => (
                        <Tile
                          key={p.id}
                          p={p}
                          mini
                          onOpen={() => setLb({ items: m.photos, index: i })}
                          onContextMenu={(e) =>
                            menu.onContextMenu(e, p, {
                              onOpen: () => setLb({ items: m.photos, index: i }),
                              onFavoriteChanged: favoriteChanged,
                            })
                          }
                        />
                      ))}
                    </div>
                  )}
                  <div className="tl-mem-line">
                    {(line || worlds) && <span>{[line, worlds].filter(Boolean).join(" — ")}</span>}
                    <button type="button" className="tl-small" onClick={() => nav.showDay(m.day)}>
                      この日を開く
                    </button>
                  </div>
                </div>
              );
            })}
          </section>
        )}
      </div>
      {lb && (
        <Lightbox
          items={lb.items}
          index={lb.index}
          onIndexChange={(i) => setLb((x) => (x ? { ...x, index: i } : x))}
          onClose={() => setLb(null)}
          onFavoriteChanged={favoriteChanged}
        />
      )}
      {menu.element}
    </div>
  );
});
