import { useEffect, useRef, type SetStateAction } from "react";
import {
  answerTrayChoice,
  applyUpdate,
  getSettings,
  getStatus,
  onAskTray,
  onImportFinished,
  onImporting,
  onSettingsChanged,
  onStatus,
  onUpdateReady,
  type Settings,
} from "./api";
import type { ChooseOptions } from "./components/Dialog";
import { useLatest } from "./components/hooks";
import { errText } from "./components/util";
import { onOpenMetadataDialog } from "./metadataApi";
import type { MetadataOpenOptions } from "./features/metadata/MetadataProvider";

const PENDING_STATUS = "新しいログを取り込みました（続きを読み込んだ一覧は「再取り込み」で更新）";

interface Options {
  setStatus(s: SetStateAction<string>): void;
  setImporting(b: boolean): void;
  setSettings(s: Settings): void;
  showError(message: string): void;
  choose(opts: ChooseOptions): Promise<string | null>;
  openMetadata(opts: MetadataOpenOptions): void;
  refreshAll(): void;
  /** Reload what an automatic import calls for; true while some list is still stale. */
  refreshAfterAutoImport(): boolean;
  hasPending(): boolean;
}

/** Subscribes to the backend's events once for the life of the app. */
export function useBackendEvents(opts: Options) {
  const o = useLatest(opts);
  const askingTray = useRef(false);

  useEffect(() => {
    let alive = true;
    const unlisten: Promise<() => void>[] = [];
    getStatus()
      .then((s) => {
        if (!alive) return;
        // Keep a status text that arrived first.
        o.current.setStatus((prev) => prev || s.status);
        o.current.setImporting(s.importing);
      })
      .catch((e) => {
        if (alive) o.current.setStatus(errText(e));
      });
    getSettings()
      .then((st) => {
        if (alive) o.current.setSettings(st);
      })
      .catch(() => {});
    unlisten.push(onStatus((s) => o.current.setStatus(s)));
    unlisten.push(
      onSettingsChanged((st) => {
        o.current.setSettings(st);
        o.current.refreshAll();
      }),
    );
    unlisten.push(
      onAskTray(() => {
        if (askingTray.current) return;
        askingTray.current = true;
        o.current
          .choose({
            title: "閉じるときの動作",
            message: "ウィンドウを閉じたあとも、タスクトレイに残してログの取り込みを続けますか？ あとから設定で変更できます。",
            buttons: [
              { id: "stay", label: "トレイに残す", primary: true },
              { id: "exit", label: "終了する" },
              { id: "cancel", label: "キャンセル" },
            ],
            cancelId: "cancel",
          })
          .then((id) => {
            if (id === "stay" || id === "exit") return answerTrayChoice(id === "stay");
          })
          .catch((e) => o.current.showError(errText(e)))
          .finally(() => {
            askingTray.current = false;
          });
      }),
    );
    unlisten.push(
      onUpdateReady(({ version }) => {
        o.current
          .choose({
            title: "更新",
            message: `新しいバージョン ${version} をダウンロードしました。再起動して更新しますか？`,
            buttons: [
              { id: "apply", label: "再起動して更新", primary: true },
              { id: "later", label: "あとで" },
            ],
            cancelId: "later",
          })
          .then((id) => {
            if (id === "apply") return applyUpdate();
          })
          .catch((e) => o.current.showError(errText(e)));
      }),
    );
    unlisten.push(onImporting((b) => o.current.setImporting(b)));
    unlisten.push(
      onOpenMetadataDialog(() =>
        o.current.openMetadata({ target: { kind: "all" }, label: "書き込めるすべての写真", allowDownload: true }),
      ),
    );
    unlisten.push(
      onImportFinished(({ changed, automatic }) => {
        if (changed && !automatic) {
          o.current.refreshAll();
        } else if (changed && automatic) {
          if (o.current.refreshAfterAutoImport()) o.current.setStatus(PENDING_STATUS);
        } else if (!changed && !automatic && o.current.hasPending()) {
          o.current.refreshAll();
        }
      }),
    );
    return () => {
      alive = false;
      unlisten.forEach((p) => p.then((f) => f()).catch(() => {}));
    };
  }, [o]);
}
