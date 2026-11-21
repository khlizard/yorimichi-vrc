import { SearchInput } from "../../components/SearchInput";
import { useEffect, useImperativeHandle, useMemo, useRef, useState, type Ref } from "react";
import {
  visitPlayers,
  visitUrls,
  visits as fetchVisits,
  type UrlHit,
  type VisitPlayerRow,
  type VisitRow,
} from "../../api";
import { describeAdd, useAddToAlbum } from "../albums/addToAlbum";
import { DataGrid, type Column } from "../../components/DataGrid";
import { useDialog } from "../../components/Dialog";
import { Splitter } from "../../components/Splitter";
import { UrlGrid } from "../../components/UrlGrid";
import { VirtualList, type ListHandle } from "../../components/VirtualList";
import { useLatest } from "../../components/hooks";
import { errText, fmtMb, nullIfBlank, type TabHandle } from "../../components/util";
import {
  createAlbumFromVisit,
  openInstance,
  openUserPage,
  openWorldPage,
  visitPlayerBadges,
  type PlayerBadge,
} from "../../libraryApi";
import { useNav } from "../../nav";
import { Lightbox } from "../photos/Lightbox";
import { logicalDay } from "../photos/photoUtil";
import { PhotoStrip, useVisitPhotos } from "../photos/PhotoStrip";
import { withFavorite } from "../photos/usePhotoSelection";

export interface SessionsHandle extends TabHandle {
  /** Select a visit by id (clears the world filter if it is not in the list) and scroll to it. */
  selectVisitById(id: number): Promise<void>;
}

function badgeTexts(b: PlayerBadge | undefined): string[] {
  if (!b) return [];
  const out: string[] = [];
  if (b.firstMet) out.push("初対面");
  if (b.daysSinceLast != null) out.push(`${b.daysSinceLast} 日ぶり`);
  return out;
}

function playerColumns(badges: Map<number, PlayerBadge>): Column<VisitPlayerRow>[] {
  return [
  { header: "名前", width: 200, text: (r) => r.displayName },
  {
    header: "バッジ",
    width: 130,
    text: (r) => badgeTexts(badges.get(r.playerId)).join(" "),
    render: (r) => badgeTexts(badges.get(r.playerId)).map((t) => (
      <span key={t} className="badge">
        {t}
      </span>
    )),
  },
  { header: "アバター", flex: 1, text: (r) => r.avatarName ?? "" },
  { header: "MB", width: 55, text: (r) => fmtMb(r.avatarSizeMb) },
  { header: "Join", width: 140, text: (r) => r.joinTime },
  { header: "Leave", width: 140, text: (r) => r.leaveTime },
  { header: "滞在", width: 80, text: (r) => r.duration },
  ];
}

export function SessionsTab({
  from,
  to,
  dayStartHour,
  ref,
}: {
  from: string | null;
  to: string | null;
  /** Start of the logical day (the settings' day boundary). */
  dayStartHour: number;
  ref?: Ref<SessionsHandle>;
}) {
  const { showError, prompt } = useDialog();
  const nav = useNav();
  const addToAlbum = useAddToAlbum();
  const range = useLatest({ from, to });
  const [badges, setBadges] = useState<Map<number, PlayerBadge>>(() => new Map());
  const [addNote, setAddNote] = useState<{ text: string; albumId: number } | null>(null);
  const [leftWidth, setLeftWidth] = useState(380);
  const [urlHeight, setUrlHeight] = useState(200);
  const [worldText, setWorldText] = useState("");
  const worldTextRef = useLatest(worldText);
  /** World term used by the last load; the debounce only reloads when the text differs. */
  const loadedTerm = useRef("");
  const [list, setList] = useState<VisitRow[]>([]);
  const listRef = useRef<VisitRow[]>([]);
  const [selectedId, setSelectedId] = useState<number | null>(null);
  const selectedIdRef = useRef<number | null>(null);
  const [players, setPlayers] = useState<VisitPlayerRow[]>([]);
  const [urls, setUrls] = useState<UrlHit[]>([]);
  const [hideSelf, setHideSelf] = useState(false);
  const gen = useRef(0);
  const detailGen = useRef(0);
  const listHandle = useRef<ListHandle>(null);
  const [visitPhotos, setVisitPhotos] = useVisitPhotos(selectedId);
  const [lbIndex, setLbIndex] = useState<number | null>(null);

  // A different visit invalidates the open photo view.
  useEffect(() => {
    setLbIndex(null);
    setAddNote(null);
  }, [selectedId]);

  const loadDetails = async (id: number | null) => {
    const g = ++detailGen.current;
    if (id == null) {
      setPlayers([]);
      setUrls([]);
      setBadges(new Map());
      return;
    }
    try {
      const [p, u, b] = await Promise.all([
        visitPlayers(id),
        visitUrls(id),
        // Badges are decoration: a failure leaves the column empty.
        visitPlayerBadges(id).catch(() => [] as PlayerBadge[]),
      ]);
      if (g !== detailGen.current) return;
      setPlayers(p);
      setUrls(u);
      setBadges(new Map(b.map((x) => [x.playerId, x])));
    } catch (e) {
      if (g !== detailGen.current) return;
      setPlayers([]);
      setUrls([]);
      setBadges(new Map());
      showError(errText(e));
    }
  };

  const select = (id: number | null) => {
    selectedIdRef.current = id;
    setSelectedId(id);
    void loadDetails(id);
  };

  const applyList = (rows: VisitRow[], keep: number | null, scroll: boolean) => {
    listRef.current = rows;
    setList(rows);
    const idx = Math.max(0, rows.findIndex((r) => r.id === keep));
    const found = rows.findIndex((r) => r.id === keep) >= 0;
    const row = found ? rows[idx] : rows[0];
    select(row?.id ?? null);
    if (scroll && row) {
      const i = found ? idx : 0;
      requestAnimationFrame(() => listHandle.current?.scrollToIndex(i));
    }
  };

  const reload = async () => {
    const g = ++gen.current;
    const term = worldTextRef.current.trim();
    loadedTerm.current = term;
    try {
      const rows = await fetchVisits(nullIfBlank(term), range.current.from, range.current.to);
      if (g !== gen.current) return;
      applyList(rows, selectedIdRef.current, false);
    } catch (e) {
      if (g === gen.current) showError(errText(e));
    }
  };

  const selectVisitById = async (id: number) => {
    const inList = listRef.current.some((r) => r.id === id);
    if (inList) {
      select(id);
      const i = listRef.current.findIndex((r) => r.id === id);
      requestAnimationFrame(() => listHandle.current?.scrollToIndex(i));
      return;
    }
    const g = ++gen.current;
    // Clear the filter without triggering a debounced refresh (loadedTerm is already "").
    loadedTerm.current = "";
    setWorldText("");
    try {
      const rows = await fetchVisits(null, null, null);
      if (g !== gen.current) return;
      const found = rows.some((r) => r.id === id);
      listRef.current = rows;
      setList(rows);
      if (found) {
        select(id);
        const i = rows.findIndex((r) => r.id === id);
        requestAnimationFrame(() => requestAnimationFrame(() => listHandle.current?.scrollToIndex(i)));
      } else {
        select(rows[0]?.id ?? null);
      }
    } catch (e) {
      if (g === gen.current) showError(errText(e));
    }
  };

  // Live world filter, 300 ms debounce.
  useEffect(() => {
    if (worldText.trim() === loadedTerm.current) return;
    const id = window.setTimeout(() => void reload(), 300);
    return () => window.clearTimeout(id);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [worldText]);

  useImperativeHandle(ref, () => ({
    refresh: () => void reload(),
    selectVisitById,
    getView: () => ({ visitId: selectedIdRef.current }),
    restoreView: (v: unknown) => {
      const id = (v as { visitId: number | null } | null)?.visitId ?? null;
      if (id != null && id !== selectedIdRef.current) void selectVisitById(id);
    },
  }));

  const visible = useMemo(() => (hideSelf ? players.filter((p) => !p.isLocal) : players), [players, hideSelf]);
  const sel = list.find((r) => r.id === selectedId) ?? null;
  const columns = useMemo(() => playerColumns(badges), [badges]);

  const fail = (e: unknown) => showError(errText(e));

  const createAlbumHere = async () => {
    if (!sel) return;
    const name = await prompt({
      title: "この写真でアルバムを作る",
      label: "アルバムの名前",
      initial: `${sel.worldName} ${sel.joinTime.slice(0, 10)}`,
      okLabel: "作成",
    });
    if (name === null) return;
    try {
      const a = await createAlbumFromVisit(sel.id, name);
      nav.showAlbum(a.id);
    } catch (e) {
      fail(e);
    }
  };

  return (
    <div className="split">
      <div className="pane" style={{ width: leftWidth }}>
        <div className="searchbar">
          <label htmlFor="sessions-world">ワールド名:</label>
          <SearchInput
            id="sessions-world"
            value={worldText}
            onChange={(e) => setWorldText(e.target.value)}
            onClear={() => setWorldText("")}
          />
        </div>
        <VirtualList<VisitRow>
          ref={listHandle}
          className="fill"
          ariaLabel="セッション一覧"
          items={list}
          getKey={(r) => r.id}
          selectedKey={selectedId}
          onSelect={(r) => select(r?.id ?? null)}
          rowHeight={42}
          renderRow={(r) => (
            <div className="item">
              <div className="line ellipsis muted">{r.joinTime}</div>
              <div className="line ellipsis">
                <b>{r.worldName}</b> <span className="muted">({r.playerCount}人)</span> <span className="muted">{r.duration}</span>
              </div>
            </div>
          )}
        />
      </div>
      <Splitter direction="col" label="一覧の幅" onDelta={(d) => setLeftWidth((w) => Math.max(200, Math.min(700, w + d)))} />
      <div className="pane grow column">
        <div className="visit-header">
          {sel && (
            <>
              <div className="visit-title">{sel.worldName}</div>
              <div className="muted wrap">{sel.location}</div>
              <div className="muted">
                {sel.joinTime} 〜 {sel.leaveTime} ({sel.duration})
              </div>
            </>
          )}
          {sel && (
            <div className="visit-actions">
              {sel.worldId && (
                <button type="button" onClick={() => openWorldPage(sel.worldId!).catch(fail)}>
                  ワールドのページ
                </button>
              )}
              <button
                type="button"
                title="VRChat のサイトでインスタンスを開きます。インスタンスが残っていない場合は入れません。"
                onClick={() => openInstance(sel.location).catch(fail)}
              >
                このインスタンスを開く
              </button>
              {sel.worldRefId != null && (
                <button type="button" onClick={() => nav.showWorld(sel.worldRefId!)}>
                  ワールドの情報
                </button>
              )}
              <button type="button" onClick={() => nav.showDay(logicalDay(sel.joinTime, dayStartHour))}>
                この日のタイムライン
              </button>
            </div>
          )}
          <label className="check">
            <input type="checkbox" checked={hideSelf} onChange={(e) => setHideSelf(e.target.checked)} />
            自分を隠す
          </label>
        </div>
        <PhotoStrip
          photos={visitPhotos}
          onOpen={setLbIndex}
          onFavoriteChanged={(id, fav) => setVisitPhotos((l) => withFavorite(l, id, fav))}
          actions={
            <>
              <button type="button" onClick={() => void createAlbumHere()}>
                この写真でアルバムを作る…
              </button>
              {addNote && (
                <span role="status">
                  {addNote.text}{" "}
                  <button type="button" onClick={() => nav.showAlbum(addNote.albumId)}>
                    アルバムを開く
                  </button>
                </span>
              )}
            </>
          }
        />
        <div className="grow-box">
          <DataGrid<VisitPlayerRow>
            className="fill"
            ariaLabel="同席プレイヤー"
            items={visible}
            getKey={(_, i) => i}
            columns={columns}
            rowClassName={(r) => (r.isLocal ? "local" : "")}
            getMenu={(r) => [
              { label: "VRChat のユーザーページを開く", onClick: () => void openUserPage(r.userId).catch(fail) },
              { label: "この人のページへ", onClick: () => nav.showPlayer(r.playerId, r.displayName) },
            ]}
          />
        </div>
        <Splitter direction="row" label="URL 欄の高さ" onDelta={(d) => setUrlHeight((h) => Math.max(60, Math.min(600, h - d)))} />
        <div className="section-label">URLイベント</div>
        <div className="fixed-box" style={{ height: urlHeight }}>
          <UrlGrid className="fill" ariaLabel="URLイベント" rows={urls} withWorld={false} />
        </div>
      </div>
      {lbIndex !== null && (
        <Lightbox
          items={visitPhotos}
          index={lbIndex}
          onIndexChange={setLbIndex}
          onClose={() => setLbIndex(null)}
          visitLink="close"
          onFavoriteChanged={(id, fav) => setVisitPhotos((l) => withFavorite(l, id, fav))}
          extraActions={[
            {
              label: "アルバムに追加…",
              onClick: (ph) =>
                void addToAlbum([ph.id]).then((r) => {
                  if (r) setAddNote({ text: describeAdd(r), albumId: r.albumId });
                }),
            },
          ]}
        />
      )}
    </div>
  );
}
