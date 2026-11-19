import { useImperativeHandle, useRef, useState, type Ref } from "react";
import { ALL, RAW_PAGE_SIZE, searchRawEvents, type RawEventHit } from "../api";
import { DataGrid, type Column } from "../components/DataGrid";
import { useDialog } from "../components/Dialog";
import { SearchInput } from "../components/SearchInput";
import { useLatest } from "../components/hooks";
import { errText, nullIfAll, type TabHandle } from "../components/util";

const LEVELS = [ALL, "Debug", "Warning"];

const columns: Column<RawEventHit>[] = [
  { header: "時刻", width: 150, text: (r) => r.eventTime },
  { header: "Lv", width: 70, text: (r) => r.level },
  { header: "ワールド", width: 180, text: (r) => r.worldName },
  { header: "メッセージ", flex: 1, text: (r) => r.message },
];

export function LogsTab({
  from,
  to,
  onNavigate,
  ref,
}: {
  from: string | null;
  to: string | null;
  onNavigate: (visitId: number | null) => void;
  ref?: Ref<TabHandle>;
}) {
  const { showError } = useDialog();
  const range = useLatest({ from, to });
  const [text, setText] = useState("");
  const [level, setLevel] = useState(ALL);
  const textRef = useLatest(text);
  const levelRef = useLatest(level);
  const active = useRef({ term: "", level: ALL });
  const [rows, setRows] = useState<RawEventHit[]>([]);
  const rowsRef = useRef<RawEventHit[]>([]);
  const [hasMore, setHasMore] = useState(false);
  const gen = useRef(0);

  const setAll = (list: RawEventHit[]) => {
    rowsRef.current = list;
    setRows(list);
  };

  const fetchPage = (last: RawEventHit | undefined) =>
    searchRawEvents(
      active.current.term,
      nullIfAll(active.current.level, ALL),
      range.current.from,
      range.current.to,
      last?.eventTime ?? null,
      last?.id ?? null,
      RAW_PAGE_SIZE,
    );

  const reload = async () => {
    const g = ++gen.current;
    try {
      const page = await fetchPage(undefined);
      if (g !== gen.current) return;
      setAll(page);
      setHasMore(page.length === RAW_PAGE_SIZE);
    } catch (e) {
      if (g === gen.current) showError(errText(e));
    }
  };

  const loadMore = async () => {
    const g = gen.current;
    try {
      const page = await fetchPage(rowsRef.current[rowsRef.current.length - 1]);
      if (g !== gen.current) return;
      setAll([...rowsRef.current, ...page]);
      setHasMore(page.length === RAW_PAGE_SIZE);
    } catch (e) {
      if (g === gen.current) showError(errText(e));
      throw e;
    }
  };

  const search = (levelOverride?: string) => {
    active.current = { term: textRef.current.trim(), level: levelOverride ?? levelRef.current };
    void reload();
  };

  useImperativeHandle(ref, () => ({ refresh: () => void reload(), loadedCount: () => rowsRef.current.length }));

  return (
    <div className="tab-col">
      <div className="searchbar">
        <SearchInput
          className="grow"
          aria-label="生ログ検索"
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
        <select
          aria-label="レベル"
          style={{ width: 90 }}
          value={level}
          onChange={(e) => {
            setLevel(e.target.value);
            search(e.target.value);
          }}
        >
          {LEVELS.map((l) => (
            <option key={l} value={l}>
              {l}
            </option>
          ))}
        </select>
        <button onClick={() => search()}>検索</button>
      </div>
      <div className="grow-box">
        <DataGrid<RawEventHit>
          className="fill"
          ariaLabel="生ログ"
          keepSelection
          items={rows}
          getKey={(r) => r.id}
          columns={columns}
          onActivate={(r) => onNavigate(r.visitId)}
          infinite={{ hasMore, loadMore, itemCount: rows.length }}
        />
      </div>
    </div>
  );
}
