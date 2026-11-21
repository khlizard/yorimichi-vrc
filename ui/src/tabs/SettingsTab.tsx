import { useEffect, useState } from "react";
import {
  autostartGet,
  backupNow,
  checkUpdate,
  defaultNoisePatterns,
  getSettings,
  getStatus,
  onSettingsChanged,
  onStatus,
  pickFolder,
  reimportAll,
  saveSettings,
  vacuum,
  type Settings,
  type Status,
} from "../api";
import { useDialog } from "../components/Dialog";
import { errText } from "../components/util";
import { applyTheme, normalizeTheme, type ThemeSetting } from "../theme";

const mb = (bytes: number) => Math.round(bytes / 1024 / 1024);
const clamp = (n: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, n));

/** Folder list with add / remove, shared by the log and photo folder settings. */
function FolderList(props: { label: string; pickTitle: string; folders: string[]; onChange: (f: string[]) => void }) {
  const { showError } = useDialog();
  const [selected, setSelected] = useState<number | null>(null);
  const add = async () => {
    try {
      const dir = await pickFolder(props.pickTitle);
      if (dir != null) props.onChange([...props.folders, dir]);
    } catch (e) {
      showError(errText(e));
    }
  };
  const remove = () => {
    if (selected == null) return;
    props.onChange(props.folders.filter((_, i) => i !== selected));
    setSelected(null);
  };
  return (
    <>
      <ul className="folder-list" role="listbox" aria-label={props.label} tabIndex={0}>
        {props.folders.map((f, i) => (
          <li
            key={`${i}:${f}`}
            role="option"
            aria-selected={selected === i}
            className={selected === i ? "selected" : ""}
            title={f}
            onClick={() => setSelected(i)}
          >
            {f}
          </li>
        ))}
      </ul>
      <div className="button-row">
        <button onClick={() => void add()}>追加...</button>
        <button onClick={remove} disabled={selected == null}>
          削除
        </button>
      </div>
    </>
  );
}

/** The settings form (shown in its own window). `onClose` is wired to the 閉じる button. */
export function SettingsTab({ onClose }: { onClose: () => void }) {
  const { showError } = useDialog();
  const [s, setS] = useState<Settings | null>(null);
  const [autostart, setAutostart] = useState(false);
  const [patterns, setPatterns] = useState("");
  const [saveStatus, setSaveStatus] = useState("");
  const [status, setStatus] = useState<Status | null>(null);
  const [lastBackupAt, setLastBackupAt] = useState<string | null>(null);

  // Load once per window open. Later "settings-changed" events (including our own saves) must not clobber edits.
  useEffect(() => {
    let alive = true;
    (async () => {
      try {
        const [st, auto] = await Promise.all([getSettings(), autostartGet()]);
        if (!alive) return;
        setS(st);
        setLastBackupAt(st.LastBackupAt);
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

  // DB size / version, refreshed whenever a status text arrives (vacuum, import and backup all report through it).
  useEffect(() => {
    let alive = true;
    const refresh = () =>
      getStatus()
        .then((st) => {
          if (alive) setStatus(st);
        })
        .catch(() => {});
    refresh();
    // Only LastBackupAt (backend-owned) and Theme (the window menu can change it) are taken from the event; the rest of the form is the user's draft.
    const un = [
      onStatus(refresh),
      onSettingsChanged((n) => {
        if (!alive) return;
        setLastBackupAt(n.LastBackupAt);
        setS((prev) => (prev ? { ...prev, Theme: normalizeTheme(n.Theme) } : prev));
      }),
    ];
    return () => {
      alive = false;
      un.forEach((p) => p.then((f) => f()).catch(() => {}));
    };
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

  const save = async () => {
    const toSave: Settings = {
      ...s,
      AutoImportIntervalMinutes: Math.max(0, s.AutoImportIntervalMinutes),
      RawLogRetentionDays: Math.max(0, s.RawLogRetentionDays),
      RawLogNoisePatterns: patterns
        .split(/\r?\n/)
        .map((l) => l.replace(/\s+$/, ""))
        .filter((l) => l.trim() !== ""),
      DayStartHour: clamp(s.DayStartHour, 0, 23),
      ReunionDays: Math.max(0, s.ReunionDays),
      AutoVacuumMinDbMb: Math.max(0, s.AutoVacuumMinDbMb),
      BackupIntervalDays: Math.max(0, s.BackupIntervalDays),
      BackupKeepCount: Math.max(1, s.BackupKeepCount),
    };
    try {
      const text = await saveSettings(toSave, autostart);
      setS(toSave);
      setSaveStatus(text);
      getStatus()
        .then(setStatus)
        .catch(() => {});
    } catch (e) {
      showError(errText(e));
    }
  };

  const pickBackupFolder = async () => {
    try {
      const dir = await pickFolder("バックアップ先フォルダを選択");
      if (dir != null) patch({ BackupFolder: dir });
    } catch (e) {
      showError(errText(e));
    }
  };

  const reclaimMb = status ? mb(status.reclaimableBytes) : 0;
  const recommended = status?.vacuumRecommended ?? false;

  return (
    <div className="settings-window">
      <div className="settings">
        <div className="settings-inner">
          <div className="section-label">ログ</div>
          <FolderList
            label="ログフォルダ"
            pickTitle="ログフォルダを選択"
            folders={s.LogFolders}
            onChange={(f) => patch({ LogFolders: f })}
          />
          <label htmlFor="db-path" className="field-label">
            データベースファイル
          </label>
          <input id="db-path" type="text" value={s.DbPath} onChange={(e) => patch({ DbPath: e.target.value })} />
          <div className="hint">相対パスは設定ファイルのあるフォルダ基準。%VAR% 展開可。</div>
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
          <textarea
            id="noise-patterns"
            className="patterns"
            spellCheck={false}
            value={patterns}
            onChange={(e) => setPatterns(e.target.value)}
          />
          <div className="button-row">
            <button onClick={run(async () => setPatterns((await defaultNoisePatterns()).join("\n")))}>既定に戻す</button>
            <button
              onClick={run(reimportAll)}
              title="取り込み済みのログファイルをすべて解析し直します (通知など新しく対応した項目を過去分にも反映)"
            >
              全ログを再取り込み
            </button>
          </div>

          <div className="section-label">写真</div>
          <FolderList
            label="写真フォルダ"
            pickTitle="写真フォルダを選択"
            folders={s.PhotoFolders}
            onChange={(f) => patch({ PhotoFolders: f })}
          />
          <div className="footnote">フォルダの中を下の階層まで探します。写真は読み込むだけで、移動・変更・削除はしません。</div>
          <div className="inline">
            <label htmlFor="day-start">1 日の区切り (時)</label>
            <input
              id="day-start"
              type="number"
              className="num"
              min={0}
              max={23}
              value={s.DayStartHour}
              onChange={(e) => patch({ DayStartHour: clamp(num(e.target.value, s.DayStartHour), 0, 23) })}
            />
          </div>
          <div className="footnote">この時刻までは前日として扱います。</div>

          <div className="section-label">表示</div>
          <div className="inline">
            <label htmlFor="theme">テーマ</label>
            <select
              id="theme"
              value={normalizeTheme(s.Theme)}
              onChange={(e) => {
                const Theme = normalizeTheme(e.target.value) as ThemeSetting;
                patch({ Theme });
                applyTheme(Theme);
              }}
            >
              <option value="auto">自動 (Windows の設定に合わせる)</option>
              <option value="light">ライト</option>
              <option value="dark">ダーク</option>
            </select>
          </div>
          <label className="check">
            <input type="checkbox" checked={s.ShowFirstMet} onChange={(e) => patch({ ShowFirstMet: e.target.checked })} />
            初めて会った人にバッジを付ける
          </label>
          <div className="inline">
            <input
              id="reunion-days"
              type="number"
              className="num"
              min={0}
              value={s.ReunionDays}
              onChange={(e) => patch({ ReunionDays: num(e.target.value, s.ReunionDays) })}
            />
            <label htmlFor="reunion-days">久しぶりに会った人にバッジを付ける (日数、0 で付けない)</label>
          </div>

          <div className="section-label">常駐と起動</div>
          <label className="check">
            <input type="checkbox" checked={s.MinimizeToTray} onChange={(e) => patch({ MinimizeToTray: e.target.checked })} />
            閉じるボタンでタスクトレイに残す
          </label>
          <label className="check">
            <input type="checkbox" checked={autostart} onChange={(e) => setAutostart(e.target.checked)} />
            Windows のサインイン時に、トレイに格納した状態で起動する
          </label>

          <div className="section-label">データベース</div>
          <div>
            現在のサイズ: {status ? `${mb(status.dbSizeBytes)} MB` : "-"} / 最適化で減らせる量:{" "}
            {status ? `約 ${reclaimMb} MB` : "-"}
          </div>
          <label className="check">
            <input type="checkbox" checked={s.AutoVacuum} onChange={(e) => patch({ AutoVacuum: e.target.checked })} />
            PC を使っていないときに自動で最適化する
          </label>
          <div className="inline">
            <input
              id="vacuum-min"
              type="number"
              className="num-wide"
              min={0}
              value={s.AutoVacuumMinDbMb}
              onChange={(e) => patch({ AutoVacuumMinDbMb: num(e.target.value, s.AutoVacuumMinDbMb) })}
            />
            <label htmlFor="vacuum-min">MB 以上のときに自動で最適化する</label>
          </div>
          <div className="button-row">
            <button className={recommended ? "button-accent" : undefined} onClick={run(vacuum)}>
              DB を最適化 (VACUUM)
            </button>
            <span className={recommended ? undefined : "muted"}>
              {recommended ? `約 ${reclaimMb} MB 減らせます` : `空きは約 ${reclaimMb} MB です`}
            </span>
          </div>

          <div className="section-label">バックアップ</div>
          <label htmlFor="backup-folder" className="field-label">
            バックアップ先フォルダ
          </label>
          <div className="inline">
            <input
              id="backup-folder"
              type="text"
              className="grow"
              value={s.BackupFolder}
              onChange={(e) => patch({ BackupFolder: e.target.value })}
            />
            <button onClick={() => void pickBackupFolder()}>参照...</button>
          </div>
          <div className="hint">相対パスは設定ファイルのあるフォルダ基準。</div>
          <div className="inline">
            <input
              type="number"
              className="num"
              aria-label="自動バックアップの間隔 (日)"
              min={0}
              value={s.BackupIntervalDays}
              onChange={(e) => patch({ BackupIntervalDays: num(e.target.value, s.BackupIntervalDays) })}
            />
            <span>日ごとに自動バックアップ (0 で自動バックアップしない)</span>
          </div>
          <div className="inline">
            <input
              type="number"
              className="num"
              aria-label="バックアップの保持数"
              min={1}
              value={s.BackupKeepCount}
              onChange={(e) => patch({ BackupKeepCount: num(e.target.value, s.BackupKeepCount) })}
            />
            <span>個まで残す (古いものから削除)</span>
          </div>
          <div>最後のバックアップ: {lastBackupAt ?? "まだありません"}</div>
          <div className="button-row">
            <button onClick={run(backupNow)}>今すぐバックアップ</button>
          </div>

          <div className="section-label">更新</div>
          <div>現在のバージョン: {status?.version ?? "-"}</div>
          <label htmlFor="update-feed" className="field-label">
            更新の配布元 (URL またはフォルダ)
          </label>
          <input id="update-feed" type="text" value={s.UpdateFeed} onChange={(e) => patch({ UpdateFeed: e.target.value })} />
          <div className="footnote">空欄のときは更新を確認しません。</div>
          <div className="button-row">
            <button onClick={run(checkUpdate)}>更新を確認</button>
          </div>
        </div>
      </div>
      <div className="settings-footer">
        <button onClick={() => void save()}>保存</button>
        <span className="save-status" role="status">
          {saveStatus}
        </span>
        <button onClick={onClose}>閉じる</button>
      </div>
    </div>
  );
}
