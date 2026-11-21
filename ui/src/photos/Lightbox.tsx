import { useCallback, useEffect, useRef, useState } from "react";
import { fullUrl, openPhoto, photoDetail, revealPhoto, setPhotoFavorite, thumbUrl, type PhotoDetail, type PhotoRow } from "../api";
import { useDialog } from "../components/Dialog";
import { useLatest } from "../components/hooks";
import { errText } from "../components/util";
import { isShown, useBackIntercept } from "../nav";
import { StarIcon } from "../components/Star";
import { photoLabel } from "./photoUtil";
import { usePhotoMenu } from "./usePhotoMenu";
import "./photos.css";

export interface LightboxAction {
  label: string;
  onClick: (photo: PhotoRow) => void;
}

export interface LightboxProps {
  items: PhotoRow[];
  index: number;
  onIndexChange: (index: number) => void;
  onClose: () => void;
  onNavigateVisit?: (visitId: number) => void;
  onNavigatePlayer?: (playerId: number, displayName: string) => void;
  onFavoriteChanged?: (id: number, favorite: boolean) => void;
  /** Called when the user gets within a few photos of the end of `items` (to load more). */
  onNearEnd?: () => void;
  extraActions?: LightboxAction[];
}

const FOCUSABLE = "button:not(:disabled), [href], input:not(:disabled), select:not(:disabled), [tabindex]:not([tabindex='-1'])";

export function Lightbox(p: LightboxProps) {
  const { items, index } = p;
  const { showError } = useDialog();
  const photo: PhotoRow | undefined = items[index];
  const id = photo?.id ?? -1;
  const latest = useLatest(p);
  const rootRef = useRef<HTMLDivElement>(null);
  const idRef = useLatest(id);
  const [showInfo, setShowInfo] = useState(true);
  const [loadedId, setLoadedId] = useState<number | null>(null);
  const [badId, setBadId] = useState<number | null>(null);
  const [thumbBadId, setThumbBadId] = useState<number | null>(null);
  const [detail, setDetail] = useState<{ id: number; d: PhotoDetail | null; failed: boolean } | null>(null);
  const [favOv, setFavOv] = useState<Map<number, boolean>>(() => new Map());
  const favBusy = useRef(false);
  const preload = useRef<HTMLImageElement[]>([]);
  const menu = usePhotoMenu();

  useBackIntercept(() => {
    if (!isShown(rootRef.current)) return false;
    latest.current.onClose();
    return true;
  });

  // Focus the dialog on open; give focus back to the opening tile (or the tile of the current photo) on close.
  useEffect(() => {
    const opener = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    rootRef.current?.focus();
    return () => {
      if (opener?.isConnected) opener.focus();
      else document.querySelector<HTMLElement>(`[data-photo-id="${idRef.current}"]`)?.focus();
    };
  }, [idRef]);

  // Photo vanished from the list (reload): leave quietly.
  useEffect(() => {
    if (!photo) p.onClose();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [photo]);

  useEffect(() => {
    if (index >= items.length - 3) latest.current.onNearEnd?.();
  }, [index, items.length, latest]);

  // Per-photo detail with a stale-response guard.
  useEffect(() => {
    if (id < 0) return;
    let alive = true;
    photoDetail(id).then(
      (d) => alive && setDetail({ id, d, failed: false }),
      () => alive && setDetail({ id, d: null, failed: true }),
    );
    return () => {
      alive = false;
    };
  }, [id]);

  // Preload the neighbours only after the current full image has loaded.
  useEffect(() => {
    if (loadedId !== id) return;
    const imgs: HTMLImageElement[] = [];
    for (const n of [index + 1, index - 1]) {
      const q = items[n];
      if (!q || q.onlineOnly) continue;
      const im = new Image();
      im.src = fullUrl(q.id);
      imgs.push(im);
    }
    preload.current = imgs;
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [loadedId, id, index]);

  const go = useCallback(
    (delta: number) => {
      const { items: its, index: i, onIndexChange } = latest.current;
      const n = i + delta;
      if (n >= 0 && n < its.length) onIndexChange(n);
    },
    [latest],
  );

  const fav = photo ? (favOv.get(photo.id) ?? photo.favorite) : false;
  const toggleFav = async () => {
    if (!photo || favBusy.current) return;
    const pid = photo.id;
    const next = !fav;
    favBusy.current = true;
    setFavOv((m) => new Map(m).set(pid, next));
    try {
      await setPhotoFavorite(pid, next);
      p.onFavoriteChanged?.(pid, next);
    } catch (e) {
      setFavOv((m) => new Map(m).set(pid, !next));
      showError(errText(e));
    } finally {
      favBusy.current = false;
    }
  };

  const run = (f: () => Promise<void>) => {
    f().catch((e) => showError(errText(e)));
  };

  const onKeyDown = (e: React.KeyboardEvent) => {
    if (e.ctrlKey || e.metaKey || e.altKey) return;
    switch (e.key) {
      case "Escape":
        e.preventDefault();
        e.stopPropagation();
        p.onClose();
        return;
      case "ArrowLeft":
        e.preventDefault();
        go(-1);
        return;
      case "ArrowRight":
        e.preventDefault();
        go(1);
        return;
      case "f":
      case "F":
        e.preventDefault();
        void toggleFav();
        return;
      case "i":
      case "I":
        e.preventDefault();
        setShowInfo((s) => !s);
        return;
      case "Tab": {
        const root = rootRef.current;
        if (!root) return;
        const list = Array.from(root.querySelectorAll<HTMLElement>(FOCUSABLE));
        if (list.length === 0) {
          e.preventDefault();
          return;
        }
        const first = list[0];
        const last = list[list.length - 1];
        const cur = document.activeElement;
        if (e.shiftKey && (cur === first || cur === root)) {
          e.preventDefault();
          last.focus();
        } else if (!e.shiftKey && cur === last) {
          e.preventDefault();
          first.focus();
        }
        return;
      }
    }
  };

  if (!photo) return null;

  const onImageMenu = (e: React.MouseEvent) =>
    menu.onContextMenu(e, photo, {
      onFavoriteChanged: (pid, next) => {
        setFavOv((m) => new Map(m).set(pid, next));
        p.onFavoriteChanged?.(pid, next);
      },
      extra: p.extraActions?.map((a) => ({ label: a.label, onClick: () => a.onClick(photo) })),
    });

  const cur = detail?.id === id ? detail : null;
  const players = cur?.d?.players ?? [];
  const failed = badId === id;
  const loaded = loadedId === id;
  const time = photo.takenAt.slice(0, 16) + (photo.timeSource === "mtime" ? "（推定）" : "");
  const visitId = photo.visitId;
  const world = photo.worldName;

  return (
    <div
      ref={rootRef}
      className="lb photo-surface"
      role="dialog"
      aria-modal="true"
      aria-label="写真"
      tabIndex={-1}
      onKeyDown={onKeyDown}
    >
      <div className="lb-bar">
        <div className="lb-group">
          <button type="button" onClick={() => go(-1)} disabled={index <= 0} title="前へ (←)">
            前へ
          </button>
          <button type="button" onClick={() => go(1)} disabled={index >= items.length - 1} title="次へ (→)">
            次へ
          </button>
          <span className="lb-pos">
            {index + 1} / {items.length.toLocaleString("ja-JP")}
          </span>
        </div>
        <div className="lb-group">
          <button type="button" aria-pressed={fav} onClick={() => void toggleFav()} title="お気に入りの切り替え (F)">
            <span className="lb-star">
              <StarIcon filled={fav} size={14} />
            </span>
            お気に入り
          </button>
          <button type="button" onClick={() => run(() => revealPhoto(photo.id))} title="エクスプローラーでこのファイルを選択して表示">
            フォルダで表示
          </button>
          <button type="button" onClick={() => run(() => openPhoto(photo.id))} title="既定のアプリで開く">
            既定のアプリで開く
          </button>
          {p.extraActions?.map((a) => (
            <button key={a.label} type="button" onClick={() => a.onClick(photo)}>
              {a.label}
            </button>
          ))}
          <button type="button" aria-pressed={showInfo} onClick={() => setShowInfo((s) => !s)} title="情報の表示 / 非表示 (I)">
            情報
          </button>
          <button type="button" onClick={p.onClose} title="閉じる (Esc)">
            閉じる
          </button>
        </div>
      </div>
      <div className="lb-stage">
        {!loaded && thumbBadId !== id && (
          <img
            key={`t${id}`}
            className="lb-img"
            src={thumbUrl(id)}
            alt=""
            draggable={false}
            onError={() => setThumbBadId(id)}
            onContextMenu={onImageMenu}
          />
        )}
        <img
          key={`f${id}`}
          className={`lb-img${loaded ? "" : " lb-hidden"}`}
          src={fullUrl(id)}
          alt={photoLabel(photo)}
          draggable={false}
          onLoad={() => setLoadedId(id)}
          onError={() => setBadId(id)}
          onContextMenu={onImageMenu}
        />
        {failed && thumbBadId === id && (
          <div className="lb-msg" onContextMenu={onImageMenu}>
            {photo.onlineOnly ? "未ダウンロード" : "表示できません"}
          </div>
        )}
      </div>
      {showInfo && (
        <div className="lb-info">
          {failed && (
            <div className="lb-note" role="status">
              {photo.onlineOnly
                ? "オンライン専用のファイルのため、縮小した画像を表示しています。「既定のアプリで開く」でダウンロードして開けます。"
                : "画像を読み込めませんでした。"}
            </div>
          )}
          <div className="lb-line">
            <span className="lb-time">{time}</span>
            {visitId != null ? (
              p.onNavigateVisit ? (
                <button type="button" className="lb-link" title="この時のセッションを開く" onClick={() => p.onNavigateVisit?.(visitId)}>
                  {world || "（ワールド名なし）"}
                </button>
              ) : (
                <span>{world}</span>
              )
            ) : (
              <>
                {world && <span>{world}</span>}
                <span className="lb-muted">この時刻のログはありません</span>
              </>
            )}
          </div>
          {visitId != null && (
            <div className="lb-people">
              <span className="lb-muted">この場にいた人{cur?.d ? ` (${players.length})` : ""}:</span>
              {!cur && <span className="lb-muted">読み込み中…</span>}
              {cur?.failed && <span className="lb-muted">取得できませんでした</span>}
              {players.map((pl) =>
                p.onNavigatePlayer ? (
                  <button
                    key={pl.playerId}
                    type="button"
                    className="lb-name"
                    title="このプレイヤーを探す"
                    onClick={() => p.onNavigatePlayer?.(pl.playerId, pl.displayName)}
                  >
                    {pl.displayName}
                    {pl.isLocal ? " (自分)" : ""}
                  </button>
                ) : (
                  <span key={pl.playerId} className="lb-name">
                    {pl.displayName}
                    {pl.isLocal ? " (自分)" : ""}
                  </span>
                ),
              )}
            </div>
          )}
        </div>
      )}
      {menu.element}
    </div>
  );
}
