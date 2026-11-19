import { useImperativeHandle, useRef, useState, type Ref } from "react";
import { searchAvatars, type AvatarRow } from "../api";
import { DataGrid, type Column } from "../components/DataGrid";
import { useDialog } from "../components/Dialog";
import { SearchInput } from "../components/SearchInput";
import { useLatest } from "../components/hooks";
import { errText, fmtMb, numText, type TabHandle } from "../components/util";

const columns: Column<AvatarRow>[] = [
  { header: "時刻", width: 150, text: (r) => r.eventTime },
  { header: "プレイヤー", width: 180, text: (r) => r.displayName },
  { header: "アバター", flex: 1, text: (r) => r.avatarName },
  { header: "サイズ (MB)", width: 90, align: "right", text: (r) => fmtMb(r.sizeMb) },
  { header: "DL 秒", width: 60, align: "right", text: (r) => numText(r.loadSeconds) },
  { header: "ワールド", width: 200, text: (r) => r.worldName },
];

export function AvatarsTab({
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
  const textRef = useLatest(text);
  const [rows, setRows] = useState<AvatarRow[]>([]);
  const gen = useRef(0);

  const search = async () => {
    const g = ++gen.current;
    try {
      const r = await searchAvatars(textRef.current.trim(), range.current.from, range.current.to);
      if (g === gen.current) setRows(r);
    } catch (e) {
      if (g === gen.current) showError(errText(e));
    }
  };

  useImperativeHandle(ref, () => ({ refresh: () => void search() }));

  return (
    <div className="tab-col">
      <div className="searchbar">
        <SearchInput
          className="grow"
          aria-label="アバター検索"
          title="アバター名またはプレイヤー名"
          value={text}
          onChange={(e) => setText(e.target.value)}
          onClear={() => {
            setText("");
            textRef.current = "";
            void search();
          }}
          onKeyDown={(e) => {
            if (e.key === "Enter") void search();
          }}
        />
        <button onClick={() => void search()}>検索</button>
      </div>
      <div className="grow-box">
        <DataGrid<AvatarRow>
          className="fill"
          ariaLabel="アバター一覧"
          items={rows}
          getKey={(_, i) => i}
          columns={columns}
          onActivate={(r) => onNavigate(r.visitId)}
        />
      </div>
      <div className="footnote">サイズはダウンロードした時だけ記録されます (キャッシュ済みのアバターは空欄)。ダブルクリックでセッションへ。</div>
    </div>
  );
}
