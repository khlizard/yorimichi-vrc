import { useEffect, useRef, useState } from "react";
import {
  autostartGet,
  defaultNoisePatterns,
  getSettings,
  pickFolder,
  reimportAll,
  saveSettings,
  vacuum,
  type Settings,
} from "../api";
import { useDialog } from "../components/Dialog";
import { errText } from "../components/util";

export function SettingsTab({ onDbPathChanged }: { onDbPathChanged: () => void }) {
  const { showError } = useDialog();
  const [s, setS] = useState<Settings | null>(null);
  const loadedDbPath = useRef("");
  const [autostart, setAutostart] = useState(false);
  const [patterns, setPatterns] = useState("");
  const [selectedFolder, setSelectedFolder] = useState<number | null>(null);
  const [saveStatus, setSaveStatus] = useState("");

  useEffect(() => {
    let alive = true;
    (async () => {
      try {
        const [st, auto] = await Promise.all([getSettings(), autostartGet()]);
        if (!alive) return;
        setS(st);
        loadedDbPath.current = st.DbPath;
        setAutostart(auto);
        setPatterns(st.RawLogNoisePatterns.join("\n"));
      } catch (e) {
        if (alive) showError(errText(e));
      }
    })();
    return () => {
      alive = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  if (!s) return <div className="settings" />;

  const patch = (p: Partial<Settings>) => setS((prev) => (prev ? { ...prev, ...p } : prev));
  const num = (v: string, prev: number) => {
    const n = parseInt(v, 10);
    return Number.isNaN(n) ? prev : n;
  };
  const run = (f: () => Promise<unknown>) => () => {
    f().catch((e) => showError(errText(e)));
  };

  const addFolder = async () => {
    try {
      const dir = await pickFolder();
      if (dir != null) patch({ LogFolders: [...s.LogFolders, dir] });
    } catch (e) {
      showError(errText(e));
    }
  };

  const removeFolder = () => {
    if (selectedFolder == null) return;
    patch({ LogFolders: s.LogFolders.filter((_, i) => i !== selectedFolder) });
    setSelectedFolder(null);
  };

  const save = async () => {
    const toSave: Settings = {
      ...s,
      AutoImportIntervalMinutes: Math.max(0, s.AutoImportIntervalMinutes),
      RawLogRetentionDays: Math.max(0, s.RawLogRetentionDays),
      RawLogNoisePatterns: patterns
        .split(/\r?\n/)
        .map((l) => l.replace(/\s+$/, ""))
        .filter((l) => l.trim() !== ""),
    };
    try {
      const text = await saveSettings(toSave, autostart);
      setS(toSave);
      setSaveStatus(text);
      if (toSave.DbPath !== loadedDbPath.current) {
        loadedDbPath.current = toSave.DbPath;
        onDbPathChanged();
      }
    } catch (e) {
      showError(errText(e));
    }
  };

  return (
    <div className="settings">
      <div className="settings-inner">
        <div className="section-title">ログフォルダ</div>
        <ul className="folder-list" role="listbox" aria-label="ログフォルダ" tabIndex={0}>
          {s.LogFolders.map((f, i) => (
            <li
              key={`${i}:${f}`}
              role="option"
              aria-selected={selectedFolder === i}
              className={selectedFolder === i ? "selected" : ""}
              title={f}
              onClick={() => setSelectedFolder(i)}
            >
              {f}
            </li>
          ))}
        </ul>
        <div className="button-row">
          <button onClick={() => void addFolder()}>追加...</button>
          <button onClick={removeFolder} disabled={selectedFolder == null}>
            削除
          </button>
        </div>

        <div className="section-title">データベースファイル</div>
        <input type="text" aria-label="データベースファイル" value={s.DbPath} onChange={(e) => patch({ DbPath: e.target.value })} />
        <div className="hint">相対パスは設定ファイルのあるフォルダ基準。%VAR% 展開可。</div>

        <div className="section-title">常駐・自動取り込み</div>
        <div className="inline">
          <input
            type="number"
            className="num"
            aria-label="自動取り込みの間隔 (分)"
            min={0}
            value={s.AutoImportIntervalMinutes}
            onChange={(e) => patch({ AutoImportIntervalMinutes: num(e.target.value, s.AutoImportIntervalMinutes) })}
          />
          <span>分ごとに自動取り込み (0 で無効)</span>
        </div>
        <label className="check">
          <input type="checkbox" checked={s.MinimizeToTray} onChange={(e) => patch({ MinimizeToTray: e.target.checked })} />
          閉じるボタンでタスクトレイに常駐する (終了はトレイの右クリックから)
        </label>
        <label className="check">
          <input type="checkbox" checked={autostart} onChange={(e) => setAutostart(e.target.checked)} />
          Windows のサインイン時に自動で起動する (トレイに格納して起動)
        </label>

        <div className="section-title">生ログの保持</div>
        <div className="inline">
          <input
            type="number"
            className="num"
            aria-label="生ログの保持日数"
            min={0}
            value={s.RawLogRetentionDays}
            onChange={(e) => patch({ RawLogRetentionDays: num(e.target.value, s.RawLogRetentionDays) })}
          />
          <span>日より古い生ログを削除 (0 で無制限)</span>
        </div>
        <div className="hint">セッション・同席プレイヤー・URL・通知は削除されません。</div>
        <label htmlFor="noise-patterns" className="field-label">
          生ログから除外する行 (1 行 1 パターン、前方一致。先頭に * を付けると部分一致)
        </label>
        <textarea id="noise-patterns" className="patterns" spellCheck={false} value={patterns} onChange={(e) => setPatterns(e.target.value)} />
        <div className="button-row">
          <button onClick={run(async () => setPatterns((await defaultNoisePatterns()).join("\n")))}>既定に戻す</button>
          <button onClick={run(vacuum)}>DB 最適化 (VACUUM)</button>
          <button
            onClick={run(reimportAll)}
            title="取り込み済みのログファイルをすべて解析し直します (通知など新しく対応した項目を過去分にも反映)"
          >
            全ログを再取り込み
          </button>
        </div>

        <div className="button-row save-row">
          <button onClick={() => void save()}>保存</button>
          <span className="save-status" role="status">
            {saveStatus}
          </span>
        </div>
      </div>
    </div>
  );
}
