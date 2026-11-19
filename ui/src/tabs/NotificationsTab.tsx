import { useImperativeHandle, useRef, useState, type Ref } from "react";
import {
  ALL,
  notificationKinds,
  searchModeration,
  searchNotifications,
  type ModerationRow,
  type NotificationRow,
} from "../api";
import { DataGrid, type Column } from "../components/DataGrid";
import { useDialog } from "../components/Dialog";
import { Splitter } from "../components/Splitter";
import { useLatest } from "../components/hooks";
import { errText, nullIfAll, type TabHandle } from "../components/util";

const INITIAL_KINDS = [ALL, "invite", "requestInvite", "friendRequest", "boop", "message"];

const notifColumns: Column<NotificationRow>[] = [
  { header: "時刻", width: 150, text: (r) => r.eventTime },
  { header: "種別", width: 110, text: (r) => r.kind },
  { header: "送信者", width: 180, text: (r) => r.senderName },
  { header: "本文", flex: 2, text: (r) => r.message },
  { header: "詳細", flex: 1, text: (r) => r.details },
  { header: "応答", width: 60, text: (r) => r.response },
];

const modColumns: Column<ModerationRow>[] = [
  { header: "時刻", width: 150, text: (r) => r.eventTime },
  { header: "種別", width: 90, text: (r) => r.kind },
  { header: "対象", width: 220, text: (r) => r.targetName },
  { header: "ワールド", flex: 1, text: (r) => r.worldName },
];

export function NotificationsTab({
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
  const [kinds, setKinds] = useState(INITIAL_KINDS);
  const textRef = useLatest(text);
  const kindRef = useLatest(kind);
  const [notifs, setNotifs] = useState<NotificationRow[]>([]);
  const [mods, setMods] = useState<ModerationRow[]>([]);
  const [modHeight, setModHeight] = useState(180);
  const gen = useRef(0);

  const search = async (kindOverride?: string) => {
    const g = ++gen.current;
    const term = textRef.current.trim();
    const { from: f, to: t } = range.current;
    try {
      const [n, m, k] = await Promise.all([
        searchNotifications(term, nullIfAll(kindOverride ?? kindRef.current, ALL), f, t),
        searchModeration(term, f, t),
        notificationKinds(),
      ]);
      if (g !== gen.current) return;
      setNotifs(n);
      setMods(m);
      // Kinds only ever get appended, never removed.
      setKinds((prev) => {
        const add = k.filter((x) => !prev.includes(x));
        return add.length ? [...prev, ...add] : prev;
      });
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
          aria-label="通知検索"
          value={text}
          onChange={(e) => setText(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") void search();
          }}
        />
        <select
          aria-label="種別"
          style={{ width: 120 }}
          value={kind}
          onChange={(e) => {
            setKind(e.target.value);
            void search(e.target.value);
          }}
        >
          {kinds.map((k) => (
            <option key={k} value={k}>
              {k}
            </option>
          ))}
        </select>
        <button onClick={() => void search()}>検索</button>
      </div>
      <div className="grow-box">
        <DataGrid<NotificationRow> className="fill" ariaLabel="通知一覧" items={notifs} getKey={(_, i) => i} columns={notifColumns} />
      </div>
      <Splitter direction="row" label="モデレーション欄の高さ" onDelta={(d) => setModHeight((h) => Math.max(60, Math.min(600, h - d)))} />
      <div className="section-label">モデレーション (ミュート / ブロック / Vote Kick)</div>
      <div className="fixed-box" style={{ height: modHeight }}>
        <DataGrid<ModerationRow>
          className="fill"
          ariaLabel="モデレーション一覧"
          items={mods}
          getKey={(_, i) => i}
          columns={modColumns}
          onActivate={(r) => onNavigate(r.visitId)}
        />
      </div>
    </div>
  );
}
