import { forwardRef, useEffect, useImperativeHandle, useRef, useState } from "react";
import { visits as fetchVisits, type VisitRow } from "../api";
import { useDialog } from "../components/Dialog";
import { Splitter } from "../components/Splitter";
import { VirtualList, type ListHandle } from "../components/VirtualList";
import { StarButton, StarIcon } from "../components/Star";
import { useLatest } from "../components/hooks";
import { errText, nullIfBlank, type TabHandle } from "../components/util";
import { openWorldPage, setWorldNote, worlds as fetchWorlds, type WorldRow, type WorldSort } from "../libraryApi";
import { useNav } from "../nav";
import { Lightbox } from "../photos/Lightbox";
import { PhotoStrip, useRecentPhotos } from "../photos/PhotoStrip";
import { fmtDur, nf } from "../timeline/format";
import "./worlds.css";

export interface WorldsHandle extends TabHandle {
  /** Clear the filters, load, select the world and scroll to it. */
  selectWorld(worldRefId: number): void;
}

const SORTS: { id: WorldSort; label: string }[] = [
  { id: "lastVisit", label: "最後に行った順" },
  { id: "visitCount", label: "回数順" },
  { id: "totalTime", label: "滞在時間順" },
  { id: "name", label: "名前順" },
];

const VISIT_CAP = 200;

/** Memos are one line; older multi-line memos are shown joined by a space. */
const oneLine = (s: string) => s.replace(/\s*[\r\n]+\s*/g, " ");

function FavMark() {
  return (
    <span className="wd-star" role="img" aria-label="お気に入り" title="お気に入り">
      <StarIcon filled size={12} />
    </span>
  );
}

export const WorldsTab = forwardRef<WorldsHandle, { active: boolean }>(function WorldsTab({ active }, ref) {
  const { showError } = useDialog();
  const nav = useNav();
  const [leftWidth, setLeftWidth] = useState(380);
  const [term, setTerm] = useState("");
  const [favOnly, setFavOnly] = useState(false);
  const [sort, setSort] = useState<WorldSort>("lastVisit");
  const [list, setList] = useState<WorldRow[]>([]);
  const [selectedId, setSelectedId] = useState<number | null>(null);
  const [draft, setDraft] = useState<{ id: number; memo: string; favorite: boolean } | null>(null);
  const [saved, setSaved] = useState(false);
  const [saving, setSaving] = useState(false);
  const [visitList, setVisitList] = useState<VisitRow[]>([]);
  const [allVisits, setAllVisits] = useState(false);
  const [lbIndex, setLbIndex] = useState<number | null>(null);
  const [favBusy, setFavBusy] = useState(false);
  const listHandle = useRef<ListHandle>(null);
  const gen = useRef(0);
  const visitGen = useRef(0);
  const started = useRef(false);
  const dirty = useRef(false);
  const selectedRef = useRef<number | null>(null);
  const filters = useLatest({ term, favOnly, sort });
  const activeRef = useLatest(active);
  /** Filter key used by the last load; the debounce only reloads when it differs. */
  const loadedKey = useRef("|false|lastVisit");
  const listRef = useRef<WorldRow[]>([]);

  const keyOf = (t: string, f: boolean, s: WorldSort) => `${t.trim()}|${f}|${s}`;

  const loadVisits = async (row: WorldRow | null) => {
    const g = ++visitGen.current;
    if (!row) {
      setVisitList([]);
      return;
    }
    try {
      const rows = await fetchVisits(row.name, null, null);
      if (g !== visitGen.current) return;
      setVisitList(rows.filter((v) => v.worldRefId === row.id));
    } catch (e) {
      if (g !== visitGen.current) return;
      setVisitList([]);
      showError(errText(e));
    }
  };

  const select = (id: number | null) => {
    selectedRef.current = id;
    setSelectedId(id);
    setDraft(null);
    setSaved(false);
    setAllVisits(false);
    setLbIndex(null);
    void loadVisits(listRef.current.find((r) => r.id === id) ?? null);
  };

  /** Fetch with the given filters. `pick` = id to select; otherwise the current selection is kept. */
  const load = async (f: { term: string; favOnly: boolean; sort: WorldSort }, pick: number | null, scroll: boolean) => {
    const g = ++gen.current;
    loadedKey.current = keyOf(f.term, f.favOnly, f.sort);
    try {
      const rows = await fetchWorlds(nullIfBlank(f.term), f.favOnly, f.sort);
      if (g !== gen.current) return;
      listRef.current = rows;
      setList(rows);
      const want = pick ?? selectedRef.current;
      const idx = rows.findIndex((r) => r.id === want);
      const row = idx >= 0 ? rows[idx] : rows[0];
      if (row && row.id === selectedRef.current && pick == null) {
        // Same selection: keep the draft, just refresh the visits.
        void loadVisits(row);
      } else {
        select(row?.id ?? null);
      }
      if (scroll && row) {
        const i = idx >= 0 ? idx : 0;
        requestAnimationFrame(() => requestAnimationFrame(() => listHandle.current?.scrollToIndex(i)));
      }
    } catch (e) {
      if (g === gen.current) showError(errText(e));
    }
  };
  const loadRef = useRef(load);
  loadRef.current = load;

  useEffect(() => {
    if (!active) return;
    if (!started.current) {
      started.current = true;
      void loadRef.current(filters.current, null, false);
    } else if (dirty.current) {
      dirty.current = false;
      void loadRef.current(filters.current, null, false);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [active]);

  // Live filtering, 300 ms debounce.
  useEffect(() => {
    if (!started.current || keyOf(term, favOnly, sort) === loadedKey.current) return;
    const id = window.setTimeout(() => void loadRef.current({ term, favOnly, sort }, null, false), 300);
    return () => window.clearTimeout(id);
  }, [term, favOnly, sort]);

  useImperativeHandle(
    ref,
    () => ({
      refresh() {
        if (!started.current) return;
        if (!activeRef.current) dirty.current = true;
        else void loadRef.current(filters.current, null, false);
      },
      selectWorld(id: number) {
        started.current = true;
        dirty.current = false;
        const f = { term: "", favOnly: false, sort: filters.current.sort };
        loadedKey.current = keyOf(f.term, f.favOnly, f.sort);
        setTerm("");
        setFavOnly(false);
        void loadRef.current(f, id, true);
      },
    }),
    [activeRef, filters],
  );

  const sel = list.find((r) => r.id === selectedId) ?? null;
  const d = sel && draft && draft.id === sel.id ? draft : null;
  const memo = d ? d.memo : oneLine(sel?.memo ?? "");
  const favorite = d ? d.favorite : (sel?.favorite ?? false);
  const [worldPhotos, setWorldPhotos] = useRecentPhotos(sel ? { worldRefId: sel.id } : null);
  const changed = sel != null && (memo !== oneLine(sel.memo) || favorite !== sel.favorite);

  // Toggling the star saves at once, together with the last saved memo (not the unsaved edit).
  const toggleFavorite = async () => {
    if (!sel || favBusy) return;
    const next = !favorite;
    setFavBusy(true);
    setList((l) => l.map((r) => (r.id === sel.id ? { ...r, favorite: next } : r)));
    listRef.current = listRef.current.map((r) => (r.id === sel.id ? { ...r, favorite: next } : r));
    setDraft((cur) => (cur && cur.id === sel.id ? { ...cur, favorite: next } : cur));
    try {
      await setWorldNote(sel.id, sel.memo, next);
    } catch (e) {
      const revert = (r: WorldRow) => (r.id === sel.id ? { ...r, favorite: !next } : r);
      setList((l) => l.map(revert));
      listRef.current = listRef.current.map(revert);
      setDraft((cur) => (cur && cur.id === sel.id ? { ...cur, favorite: !next } : cur));
      showError(errText(e));
    } finally {
      setFavBusy(false);
    }
  };

  const edit = (patch: { memo?: string; favorite?: boolean }) => {
    if (!sel) return;
    setSaved(false);
    setDraft({ id: sel.id, memo, favorite, ...patch });
  };

  const save = async () => {
    if (!sel || saving || !changed) return;
    setSaving(true);
    try {
      await setWorldNote(sel.id, memo, favorite);
      setDraft(null);
      setSaved(true);
      await loadRef.current(filters.current, null, false);
      setSaved(true);
    } catch (e) {
      showError(errText(e));
    } finally {
      setSaving(false);
    }
  };

  const shownVisits = allVisits ? visitList : visitList.slice(0, VISIT_CAP);

  return (
    <div className="split">
      <div className="pane" style={{ width: leftWidth }}>
        <div className="searchbar">
          <label htmlFor="worlds-term">検索:</label>
          <input
            id="worlds-term"
            type="text"
            placeholder="名前またはメモ"
            value={term}
            onChange={(e) => setTerm(e.target.value)}
          />
        </div>
        <div className="searchbar">
          <label className="check" style={{ marginTop: 0 }}>
            <input type="checkbox" checked={favOnly} onChange={(e) => setFavOnly(e.target.checked)} />
            お気に入りのみ
          </label>
          <label htmlFor="worlds-sort" style={{ marginLeft: "auto" }}>
            並び順:
          </label>
          <select id="worlds-sort" value={sort} onChange={(e) => setSort(e.target.value as WorldSort)}>
            {SORTS.map((s) => (
              <option key={s.id} value={s.id}>
                {s.label}
              </option>
            ))}
          </select>
        </div>
        <VirtualList<WorldRow>
          ref={listHandle}
          className="fill"
          ariaLabel="ワールド一覧"
          items={list}
          getKey={(r) => r.id}
          selectedKey={selectedId}
          onSelect={(r) => select(r?.id ?? null)}
          rowHeight={42}
          renderRow={(r) => (
            <div className="item">
              <div className="line ellipsis">
                <b>{r.name}</b>
                {r.favorite && <FavMark />}
              </div>
              <div className="line ellipsis muted">
                最後: {r.lastVisit?.slice(0, 10) ?? "-"} · {nf(r.visitCount)} 回 · {fmtDur(r.totalSeconds)} · 写真 {nf(r.photoCount)}
              </div>
            </div>
          )}
        />
      </div>
      <Splitter direction="col" label="一覧の幅" onDelta={(dx) => setLeftWidth((w) => Math.max(240, Math.min(700, w + dx)))} />
      <div className="pane grow wd">
        {!sel && <div className="wd-none">{started.current ? "該当するワールドがありません。" : ""}</div>}
        {sel && (
          <>
            <div className="wd-titlebar">
              <h2 className="wd-title">{sel.name}</h2>
              <StarButton active={sel.favorite} disabled={favBusy} onToggle={() => void toggleFavorite()} />
            </div>
            <div className="wd-row">
              {sel.worldId && (
                <button type="button" onClick={() => openWorldPage(sel.worldId).catch((e) => showError(errText(e)))}>
                  ワールドのページを開く
                </button>
              )}
            </div>
            <dl className="wd-stats">
              <div>
                <dt>初めて行った日</dt>
                <dd>{sel.firstVisit?.slice(0, 10) ?? "-"}</dd>
              </div>
              <div>
                <dt>最後に行った日</dt>
                <dd>{sel.lastVisit?.slice(0, 10) ?? "-"}</dd>
              </div>
              <div>
                <dt>回数</dt>
                <dd>{nf(sel.visitCount)} 回</dd>
              </div>
              <div>
                <dt>滞在時間</dt>
                <dd>{fmtDur(sel.totalSeconds)}</dd>
              </div>
              <div>
                <dt>写真</dt>
                <dd>{nf(sel.photoCount)} 枚</dd>
              </div>
            </dl>
            {worldPhotos.length > 0 && (
              <div className="wd-photos">
                <PhotoStrip
                  clip
                  title="写真"
                  photos={worldPhotos}
                  onOpen={setLbIndex}
                  actions={
                    <button type="button" onClick={() => nav.showPhotos({ worldRefId: sel.id, label: sel.name })}>
                      すべて見る ({nf(sel.photoCount)})
                    </button>
                  }
                />
              </div>
            )}
            <h3>メモ</h3>
            <div className="wd-memo">
              <input
                type="text"
                aria-label="ワールドのメモ"
                value={memo}
                onChange={(e) => edit({ memo: e.target.value })}
                onKeyDown={(e) => {
                  if (e.key === "Enter" && !e.nativeEvent.isComposing) {
                    e.preventDefault();
                    void save();
                  }
                }}
              />
              <button type="button" disabled={!changed || saving} onClick={() => void save()}>
                保存
              </button>
              <span className="wd-saved" role="status">
                {saved && !changed ? "保存しました" : ""}
              </span>
            </div>
            <h3>このワールドのセッション ({nf(visitList.length)})</h3>
            {visitList.length === 0 ? (
              <div className="muted">セッションはありません。</div>
            ) : (
              <>
                <ul className="wd-visits">
                  {shownVisits.map((v) => (
                    <li key={v.id}>
                      <button type="button" onClick={() => nav.showVisit(v.id)}>
                        <span>{v.joinTime.slice(0, 10)}</span>
                        <span>{v.duration}</span>
                        <span>{nf(v.playerCount)} 人</span>
                      </button>
                    </li>
                  ))}
                </ul>
                {visitList.length > VISIT_CAP && !allVisits && (
                  <button type="button" style={{ marginTop: 6 }} onClick={() => setAllVisits(true)}>
                    すべて表示 ({nf(visitList.length)})
                  </button>
                )}
              </>
            )}
          </>
        )}
      </div>
      {lbIndex !== null && (
        <Lightbox
          items={worldPhotos}
          index={lbIndex}
          onIndexChange={setLbIndex}
          onClose={() => setLbIndex(null)}
          onNavigateVisit={(id) => {
            setLbIndex(null);
            nav.showVisit(id);
          }}
          onNavigatePlayer={(id, name) => {
            setLbIndex(null);
            nav.showPlayer(id, name);
          }}
          onFavoriteChanged={(id, fav) => setWorldPhotos((l) => l.map((x) => (x.id === id ? { ...x, favorite: fav } : x)))}
        />
      )}
    </div>
  );
});
