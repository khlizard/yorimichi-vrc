import { useEffect, useImperativeHandle, useRef, useState, type Ref } from "react";
import {
  PLAYER_PAGE_SIZE,
  encounters as fetchEncounters,
  recentPlayers,
  searchPlayers,
  type EncounterRow,
  type PlayerHit,
} from "../api";
import { DataGrid, type Column } from "../components/DataGrid";
import { useDialog } from "../components/Dialog";
import { Splitter } from "../components/Splitter";
import { VirtualList } from "../components/VirtualList";
import { useLatest } from "../components/hooks";
import { errText, fmtMb, type TabHandle } from "../components/util";
import { playerMarks, playerProfile, playersByTag, listTags, type PlayerMark, type PlayerProfile, type TagCount } from "../libraryApi";
import { SearchInput } from "../components/SearchInput";
import { PlayerProfilePanel } from "../players/PlayerProfilePanel";
import { RowMarks } from "../players/tagUi";

const columns: Column<EncounterRow>[] = [
  { header: "会った日時", width: 150, text: (r) => r.joinTime },
  { header: "別れた日時", width: 150, text: (r) => r.leaveTime },
  { header: "滞在", width: 80, text: (r) => r.duration },
  { header: "名前(当時)", width: 140, text: (r) => r.displayName },
  { header: "アバター", width: 180, text: (r) => r.avatarName ?? "" },
  { header: "MB", width: 55, text: (r) => fmtMb(r.avatarSizeMb) },
  { header: "ワールド", width: 200, text: (r) => r.worldName },
  { header: "インスタンス", flex: 1, text: (r) => r.instanceId },
];

export interface PlayersHandle extends TabHandle {
  /** Search players by display name (used when jumping from a photo). */
  searchFor(name: string): void;
}

export function PlayersTab({
  from,
  to,
  onNavigate,
  ref,
}: {
  from: string | null;
  to: string | null;
  onNavigate: (visitId: number | null) => void;
  ref?: Ref<PlayersHandle>;
}) {
  const { showError } = useDialog();
  const range = useLatest({ from, to });
  const [width, setWidth] = useState(320);
  const [text, setText] = useState("");
  const textRef = useLatest(text);
  const activeTerm = useRef("");
  const [players, setPlayers] = useState<PlayerHit[]>([]);
  const playersRef = useRef<PlayerHit[]>([]);
  const [hasMore, setHasMore] = useState(false);
  const [selectedId, setSelectedId] = useState<number | null>(null);
  const selectedIdRef = useRef<number | null>(null);
  const [rows, setRows] = useState<EncounterRow[]>([]);
  const gen = useRef(0);
  const encGen = useRef(0);
  const [tags, setTags] = useState<TagCount[]>([]);
  const [tagFilter, setTagFilter] = useState<number | null>(null);
  const tagFilterRef = useRef<number | null>(null);
  const [marks, setMarks] = useState<Map<number, PlayerMark>>(() => new Map());
  const [profile, setProfile] = useState<PlayerProfile | null>(null);
  const profGen = useRef(0);

  const loadTags = async () => {
    try {
      const t = await listTags();
      setTags(t);
      // A deleted tag cannot stay as the filter.
      if (tagFilterRef.current != null && !t.some((x) => x.id === tagFilterRef.current)) {
        tagFilterRef.current = null;
        setTagFilter(null);
        void reload();
      }
    } catch (e) {
      showError(errText(e));
    }
  };

  const loadMarks = async (ids: number[]) => {
    if (ids.length === 0) return;
    try {
      const m = await playerMarks(ids);
      setMarks((prev) => {
        const next = new Map(prev);
        // Players without a mark are absent from the reply: clear any stale entry.
        for (const id of ids) next.delete(id);
        for (const x of m) next.set(x.playerId, x);
        return next;
      });
    } catch {
      // Marks are decoration; a failure must not disturb the list.
    }
  };

  const loadProfile = async (id: number | null) => {
    const g = ++profGen.current;
    if (id == null) {
      setProfile(null);
      return;
    }
    try {
      const p = await playerProfile(id);
      if (g === profGen.current) setProfile(p);
    } catch (e) {
      if (g === profGen.current) {
        setProfile(null);
        showError(errText(e));
      }
    }
  };

  useEffect(() => {
    void loadTags();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const setList = (list: PlayerHit[]) => {
    playersRef.current = list;
    setPlayers(list);
  };

  const loadEncounters = async (id: number | null) => {
    const g = ++encGen.current;
    if (id == null) {
      setRows([]);
      return;
    }
    try {
      const r = await fetchEncounters(id, range.current.from, range.current.to);
      if (g === encGen.current) setRows(r);
    } catch (e) {
      if (g === encGen.current) {
        setRows([]);
        showError(errText(e));
      }
    }
  };

  const select = (id: number | null) => {
    selectedIdRef.current = id;
    setSelectedId(id);
    void loadEncounters(id);
    void loadProfile(id);
  };

  const fetchPage = async (offset: number) => {
    const tag = tagFilterRef.current;
    if (tag != null) {
      // Tag view is unpaged; the search text narrows it on the client.
      if (offset > 0) return [];
      const all = await playersByTag(tag);
      const q = activeTerm.current.toLowerCase();
      return q === "" ? all : all.filter((p) => `${p.displayName} ${p.otherNames ?? ""}`.toLowerCase().includes(q));
    }
    return fetchPaged(offset);
  };

  const fetchPaged = (offset: number) =>
    activeTerm.current === ""
      ? recentPlayers(range.current.from, range.current.to, offset, PLAYER_PAGE_SIZE)
      : searchPlayers(activeTerm.current, offset, PLAYER_PAGE_SIZE);

  const reload = async () => {
    const g = ++gen.current;
    const keep = selectedIdRef.current;
    let page: PlayerHit[];
    try {
      page = await fetchPage(0);
    } catch (e) {
      if (g === gen.current) showError(errText(e));
      return;
    }
    if (g !== gen.current) return;
    setList(page);
    setHasMore(tagFilterRef.current == null && page.length === PLAYER_PAGE_SIZE);
    void loadMarks(page.map((p) => p.id));
    select((page.find((p) => p.id === keep) ?? page[0])?.id ?? null);
  };

  const loadMore = async () => {
    const g = gen.current;
    try {
      const page = await fetchPage(playersRef.current.length);
      if (g !== gen.current) return;
      setList([...playersRef.current, ...page]);
      setHasMore(page.length === PLAYER_PAGE_SIZE);
      void loadMarks(page.map((p) => p.id));
    } catch (e) {
      if (g === gen.current) showError(errText(e));
      throw e;
    }
  };

  const search = () => {
    activeTerm.current = textRef.current.trim();
    void reload();
  };

  useImperativeHandle(ref, () => ({
    refresh: () => {
      void reload();
      void loadTags();
    },
    loadedCount: () => playersRef.current.length,
    searchFor: (name: string) => {
      setText(name);
      tagFilterRef.current = null;
      setTagFilter(null);
      activeTerm.current = name.trim();
      selectedIdRef.current = null;
      void reload();
    },
  }));

  return (
    <div className="split">
      <div className="pane" style={{ width }}>
        <div className="searchbar">
          <SearchInput
            aria-label="プレイヤー検索"
            value={text}
            onChange={(e) => setText(e.target.value)}
            onClear={() => {
              setText("");
              textRef.current = "";
              search();
            }}
            onKeyDown={(e) => {
              if (e.key === "Enter") search();
            }}
          />
          <button onClick={search}>検索</button>
          <select
            className="tag-filter"
            aria-label="タグで絞り込む"
            value={tagFilter ?? ""}
            onChange={(e) => {
              const v = e.target.value === "" ? null : Number(e.target.value);
              tagFilterRef.current = v;
              setTagFilter(v);
              void reload();
            }}
          >
            <option value="">タグ: (全て)</option>
            {tags.map((t) => (
              <option key={t.id} value={t.id}>
                {t.name} ({t.playerCount})
              </option>
            ))}
          </select>
        </div>
        <VirtualList<PlayerHit>
          className="fill"
          ariaLabel="プレイヤー一覧"
          items={players}
          getKey={(p) => p.id}
          selectedKey={selectedId}
          onSelect={(p) => select(p?.id ?? null)}
          rowHeight={26}
          dynamic
          infinite={{ hasMore, loadMore, itemCount: players.length }}
          renderRow={(p) => (
            <div className="item">
              <div className="line ellipsis">
                <b>{p.displayName}</b> <span className="muted">({p.encounterCount}回)</span>{" "}
                <span className="faint small">{p.lastSeenDate}</span>
                <RowMarks tags={marks.get(p.id)?.tags ?? []} hasMemo={marks.get(p.id)?.hasMemo ?? false} />
              </div>
              {p.otherNames && <div className="line faint small ellipsis">aka {p.otherNames}</div>}
            </div>
          )}
        />
      </div>
      <Splitter direction="col" label="一覧の幅" onDelta={(d) => setWidth((w) => Math.max(160, Math.min(700, w + d)))} />
      <div className="pane grow">
        {profile && (
          <PlayerProfilePanel
            profile={profile}
            allTags={tags}
            onChanged={() => {
              void loadProfile(selectedIdRef.current);
              void loadMarks(playersRef.current.map((p) => p.id));
              void loadTags();
            }}
          />
        )}
        <DataGrid<EncounterRow>
          className="fill"
          ariaLabel="会った記録"
          items={rows}
          getKey={(_, i) => i}
          columns={columns}
          onActivate={(r) => onNavigate(r.visitId)}
        />
      </div>
    </div>
  );
}
