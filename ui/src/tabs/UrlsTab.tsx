import { useImperativeHandle, useRef, useState, type Ref } from "react";
import { ALL, searchUrls, type UrlHit } from "../api";
import { useDialog } from "../components/Dialog";
import { UrlGrid } from "../components/UrlGrid";
import { useLatest } from "../components/hooks";
import { errText, nullIfAll, type TabHandle } from "../components/util";

const KINDS = [ALL, "video", "string", "image"];

export function UrlsTab({
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
  const [kind, setKind] = useState(ALL);
  const textRef = useLatest(text);
  const kindRef = useLatest(kind);
  const [rows, setRows] = useState<UrlHit[]>([]);
  const gen = useRef(0);

  const search = async (kindOverride?: string) => {
    const g = ++gen.current;
    try {
      const r = await searchUrls(
        textRef.current.trim(),
        nullIfAll(kindOverride ?? kindRef.current, ALL),
        range.current.from,
        range.current.to,
      );
      if (g === gen.current) setRows(r);
    } catch (e) {
      if (g === gen.current) showError(errText(e));
    }
  };

  useImperativeHandle(ref, () => ({ refresh: () => void search() }));

  return (
    <div className="tab-col">
      <div className="searchbar">
        <input
          type="text"
          className="grow"
          aria-label="URL 検索"
          value={text}
          onChange={(e) => setText(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") void search();
          }}
        />
        <select
          aria-label="種別"
          style={{ width: 90 }}
          value={kind}
          onChange={(e) => {
            setKind(e.target.value);
            void search(e.target.value);
          }}
        >
          {KINDS.map((k) => (
            <option key={k} value={k}>
              {k}
            </option>
          ))}
        </select>
        <button onClick={() => void search()}>検索</button>
      </div>
      <div className="grow-box">
        <UrlGrid className="fill" ariaLabel="URL 一覧" rows={rows} withWorld onActivate={(r) => onNavigate(r.visitId)} />
      </div>
    </div>
  );
}
