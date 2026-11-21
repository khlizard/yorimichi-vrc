import { useEffect, useRef, useState, type KeyboardEvent, type ReactNode } from "react";
import { copyText, photoDetail, photoFileInfo, type PhotoDetail, type PhotoFileInfo, type PhotoRow } from "../api";
import { useDialog } from "../components/Dialog";
import { errText } from "../components/util";
import { albums, albumsOfPhoto } from "../libraryApi";
import { photoMetaStatus, SKIP_LABELS, type MetaStatus, type XmpState } from "../metadataApi";
import "./photos.css";

type Res<T> = { ok: true; v: T } | { ok: false };

interface Loaded {
  file: Res<PhotoFileInfo>;
  detail: Res<PhotoDetail | null>;
  meta: Res<MetaStatus>;
  albumNames: Res<string[]>;
}

async function settle<T>(p: Promise<T>): Promise<Res<T>> {
  try {
    return { ok: true, v: await p };
  } catch {
    return { ok: false };
  }
}

const nf = (n: number) => n.toLocaleString("ja-JP");
const sizeText = (b: number) => (b >= 1e6 ? `${nf(Math.round(b / 1e5) / 10)} MB` : `${nf(Math.max(1, Math.round(b / 1e3)))} KB`);

const TIME_SOURCE: Record<PhotoRow["timeSource"], string> = {
  xmp: "写真のメタデータ",
  name: "ファイル名",
  mtime: "更新日時からの推定",
};
const XMP_LABEL: Record<XmpState, string> = {
  none: "なし",
  ours: "このアプリが書き込んだもの",
  other: "VRChat などが書き込んだもの",
  unknown: "不明（オンライン専用など）",
};

function trapTab(e: KeyboardEvent<HTMLElement>) {
  if (e.key !== "Tab") return;
  const f = Array.from(e.currentTarget.querySelectorAll<HTMLElement>("button:not(:disabled)"));
  if (f.length === 0) return;
  const first = f[0];
  const last = f[f.length - 1];
  const cur = document.activeElement;
  if (e.shiftKey && (cur === first || cur === e.currentTarget)) {
    e.preventDefault();
    last.focus();
  } else if (!e.shiftKey && cur === last) {
    e.preventDefault();
    first.focus();
  }
}

function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="pi-section">
      <div className="pi-sub">{title}</div>
      <dl className="pi-dl">{children}</dl>
    </section>
  );
}
function Row({ k, children }: { k: string; children: ReactNode }) {
  return (
    <>
      <dt>{k}</dt>
      <dd>{children}</dd>
    </>
  );
}
const Failed = () => <Row k="状態">取得できませんでした</Row>;

export function PhotoInfoDialog({ photo, onClose }: { photo: PhotoRow; onClose: () => void }) {
  const { showError } = useDialog();
  const [data, setData] = useState<Loaded | null>(null);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const prev = document.activeElement;
    ref.current?.focus();
    return () => {
      if (prev instanceof HTMLElement && prev.isConnected) prev.focus();
    };
  }, []);

  useEffect(() => {
    let alive = true;
    const id = photo.id;
    const names = async () => {
      const [ids, all] = await Promise.all([albumsOfPhoto(id), albums()]);
      return ids.map((a) => all.find((x) => x.id === a)?.name ?? `アルバム ${a}`);
    };
    Promise.all([settle(photoFileInfo(id)), settle(photoDetail(id)), settle(photoMetaStatus(id)), settle(names())]).then(
      ([file, detail, meta, albumNames]) => alive && setData({ file, detail, meta, albumNames }),
    );
    return () => {
      alive = false;
    };
  }, [photo.id]);

  const copy = (t: string) => copyText(t).catch((e) => showError(errText(e)));

  const body = () => {
    if (!data)
      return (
        <div className="pi-muted" role="status">
          読み込み中…
        </div>
      );
    const { file, detail, meta, albumNames } = data;
    const d = detail.ok ? detail.v : null;
    const showWorld = !!photo.worldName || !!d?.worldId;
    return (
      <>
        <Section title="ファイル">
          {!file.ok ? (
            <Failed />
          ) : (
            <>
              <Row k="場所">{!file.v.exists ? "見つからない" : file.v.onlineOnly ? "オンライン専用" : "ローカル"}</Row>
              <Row k="フルパス">
                {file.v.path}{" "}
                <button type="button" className="pi-copy" onClick={() => void copy(file.v.path)}>
                  コピー
                </button>
              </Row>
              <Row k="フォルダ">{file.v.folder}</Row>
              {file.v.size != null && <Row k="サイズ">{sizeText(file.v.size)}</Row>}
              {photo.width && photo.height ? <Row k="画像サイズ">{`${nf(photo.width)} × ${nf(photo.height)} px`}</Row> : null}
              {file.v.modified && <Row k="更新日時">{file.v.modified}</Row>}
              {file.v.readOnly && <Row k="書き込み禁止">はい</Row>}
            </>
          )}
        </Section>
        <Section title="撮影">
          <Row k="日時">{photo.takenAt}</Row>
          <Row k="出どころ">{TIME_SOURCE[photo.timeSource]}</Row>
        </Section>
        {showWorld && (
          <Section title="ワールド">
            {photo.worldName && <Row k="名前">{photo.worldName}</Row>}
            {d?.worldId && <Row k="ワールド ID">{d.worldId}</Row>}
            {d?.instanceId && <Row k="インスタンス">{d.instanceId}</Row>}
            {d?.visitJoinTime && (
              <Row k="滞在">
                {d.visitJoinTime}〜{d.visitLeaveTime ?? ""}
              </Row>
            )}
            {!detail.ok && <Failed />}
          </Section>
        )}
        <Section title="人">
          {!detail.ok ? (
            <Failed />
          ) : (
            <>
              {d?.authorId && <Row k="撮影者">{d.authorId}</Row>}
              <Row k="その場にいた人">
                {d && d.players.length > 0 ? d.players.map((p) => p.displayName).join("、") : "記録なし"}
              </Row>
            </>
          )}
        </Section>
        <Section title="メタデータ">
          {!meta.ok ? (
            <Failed />
          ) : (
            <>
              <Row k="内容">{XMP_LABEL[meta.v.xmp]}</Row>
              <Row k="書き込み">
                {meta.v.writable
                  ? "書き込めます"
                  : meta.v.writableAfterDownload
                    ? "ダウンロードすれば書き込める可能性があります"
                    : `書き込めません${meta.v.reason ? `（${SKIP_LABELS[meta.v.reason]}）` : ""}`}
              </Row>
            </>
          )}
        </Section>
        <Section title="アルバム">
          {!albumNames.ok ? <Failed /> : <Row k="所属">{albumNames.v.length > 0 ? albumNames.v.join("、") : "なし"}</Row>}
        </Section>
      </>
    );
  };

  return (
    <div className="modal-backdrop">
      <div
        ref={ref}
        tabIndex={-1}
        className="modal pi-modal"
        role="dialog"
        aria-modal="true"
        aria-labelledby="pi-title"
        onKeyDown={(e) => {
          // The dialog owns its keys (it may sit inside a lightbox that also listens).
          e.stopPropagation();
          trapTab(e);
          if (e.key === "Escape") {
            e.preventDefault();
            onClose();
          }
        }}
      >
        <div className="modal-title pi-title" id="pi-title">
          {photo.fileName}
        </div>
        <div className="pi-body">{body()}</div>
        <div className="modal-footer">
          <button type="button" className="button-accent" onClick={onClose}>
            閉じる
          </button>
        </div>
      </div>
    </div>
  );
}
