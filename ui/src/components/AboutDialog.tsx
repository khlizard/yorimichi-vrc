import { useEffect, useRef, useState } from "react";
import { aboutInfo, applyUpdate, checkUpdate, openLicenses, openUrl, type AboutInfo, type UpdateCheck } from "../api";
import { Modal } from "./Modal";
import { errText } from "./util";

const REPO_URL = "https://github.com/khlizard/yorimichi-vrc";
const TAB = "button:not(:disabled)";

type UpdateState = { kind: "idle" } | { kind: "checking" } | UpdateCheck;

function updateText(u: UpdateState): string {
  switch (u.kind) {
    case "idle":
      return "";
    case "checking":
      return "確認しています… (新しい版があれば、続けてダウンロードします)";
    case "latest":
      return "最新版です。";
    case "ready":
      return `新しいバージョン ${u.version} をダウンロードしました。`;
    case "notInstalled":
      return "インストーラーで入れた版でだけ更新できます。";
    case "failed":
      return `更新を確認できませんでした: ${u.message}`;
  }
}

/** ヘルプ → Yorimichi について…: name, version, update check, and links. */
export function AboutDialog({ close }: { close: () => void }) {
  const [info, setInfo] = useState<AboutInfo | null>(null);
  const [update, setUpdate] = useState<UpdateState>({ kind: "idle" });
  const [error, setError] = useState("");
  const closeRef = useRef<HTMLButtonElement>(null);
  const alive = useRef(true);

  useEffect(() => {
    alive.current = true;
    aboutInfo()
      .then((i) => alive.current && setInfo(i))
      .catch((e) => alive.current && setError(errText(e)));
    return () => {
      alive.current = false;
    };
  }, []);

  const check = () => {
    setUpdate({ kind: "checking" });
    setError("");
    checkUpdate()
      .then((r) => alive.current && setUpdate(r))
      .catch((e) => alive.current && setUpdate({ kind: "failed", message: errText(e) }));
  };
  const run = (f: () => Promise<void>) => () => {
    setError("");
    f().catch((e) => alive.current && setError(errText(e)));
  };

  return (
    <Modal labelledBy="about-title" tabSelector={TAB} focusRef={closeRef} onEscape={close}>
      <div className="modal-title" id="about-title">
        Yorimichi について
      </div>
      <div className="modal-body about-body">
        <div>
          <div className="about-name">Yorimichi for VRChat</div>
          <div>バージョン {info?.version ?? "-"}</div>
        </div>
        <p className="about-note">
          VRChat の非公式ツールです。VRChat Inc. とは関係ありません。
          <br />
          Copyright (c) 2026 khlizard. MIT License.
        </p>
        <div className="about-update">
          <div className="button-row">
            <button onClick={check} disabled={update.kind === "checking"}>
              更新を確認
            </button>
            {update.kind === "ready" && (
              <button className="button-accent" onClick={run(applyUpdate)}>
                再起動して更新
              </button>
            )}
          </div>
          <div role="status">{updateText(update)}</div>
        </div>
        <div className="button-row">
          <button onClick={run(() => openUrl(REPO_URL))}>GitHub のページを開く</button>
          {info?.licensesAvailable && <button onClick={run(openLicenses)}>第三者のライセンス</button>}
        </div>
        {error && (
          <div role="alert">
            {error}
          </div>
        )}
      </div>
      <div className="modal-footer">
        <button ref={closeRef} onClick={close}>
          閉じる
        </button>
      </div>
    </Modal>
  );
}
