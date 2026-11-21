import { useCallback, useRef, useState, type MouseEvent, type ReactNode } from "react";
import {
  copyPhotoImage,
  copyText,
  openPhoto,
  photoDetail,
  revealPhoto,
  setPhotoFavorite,
  type PhotoDetail,
  type PhotoRow,
} from "../api";
import { useAddToAlbum } from "../albums/addToAlbum";
import { ContextMenu, type MenuEntry, type MenuItem } from "../components/ContextMenu";
import { useDialog } from "../components/Dialog";
import { errText } from "../components/util";
import { openWorldPage } from "../libraryApi";
import { useMetadataDialog } from "../metadata/MetadataProvider";
import { photoMetaStatus, SKIP_LABELS, type MetaStatus } from "../metadataApi";
import { useNav } from "../nav";
import { PhotoInfoDialog } from "./PhotoInfoDialog";

export interface PhotoMenuOptions {
  /** When given, the menu shows "開く" first. */
  onOpen?: () => void;
  /** Called after the favorite was changed from the menu. */
  onFavoriteChanged?: (id: number, favorite: boolean) => void;
  /** Location-specific items (e.g. "このアルバムから外す"), shown in the album/favorite group. */
  extra?: MenuItem[];
}

interface MenuState {
  x: number;
  y: number;
  photo: PhotoRow;
  opts: PhotoMenuOptions;
  status: MetaStatus | "loading" | "failed";
  detail: PhotoDetail | null;
}

export function usePhotoMenu(): {
  onContextMenu: (e: MouseEvent, photo: PhotoRow, opts?: PhotoMenuOptions) => void;
  /** Render once inside the component (context menu + info dialog). */
  element: ReactNode;
} {
  const { showError } = useDialog();
  const nav = useNav();
  const metadata = useMetadataDialog();
  const addToAlbum = useAddToAlbum();
  const [menu, setMenu] = useState<MenuState | null>(null);
  const [info, setInfo] = useState<PhotoRow | null>(null);
  const [favOv, setFavOv] = useState<Map<number, boolean>>(() => new Map());
  const seq = useRef(0);

  const closeMenu = useCallback(() => {
    seq.current++;
    setMenu(null);
  }, []);

  const onContextMenu = useCallback((e: MouseEvent, photo: PhotoRow, opts: PhotoMenuOptions = {}) => {
    e.preventDefault();
    e.stopPropagation();
    let x = e.clientX;
    let y = e.clientY;
    if (x === 0 && y === 0 && e.currentTarget instanceof Element) {
      const r = e.currentTarget.getBoundingClientRect();
      x = r.left + r.width / 2;
      y = r.top + r.height / 2;
    }
    const n = ++seq.current;
    setMenu({ x, y, photo, opts, status: "loading", detail: null });
    photoMetaStatus(photo.id).then(
      (status) => n === seq.current && setMenu((m) => (m ? { ...m, status } : m)),
      () => n === seq.current && setMenu((m) => (m ? { ...m, status: "failed" } : m)),
    );
    photoDetail(photo.id).then(
      (detail) => n === seq.current && setMenu((m) => (m ? { ...m, detail } : m)),
      () => {},
    );
  }, []);

  const run = (f: () => Promise<unknown>) => {
    f().catch((e) => showError(errText(e)));
  };

  const build = (m: MenuState): MenuEntry[] => {
    const { photo, opts, status, detail } = m;
    const id = photo.id;
    const fav = favOv.get(id) ?? photo.favorite;
    const groups: MenuItem[][] = [];

    groups.push([
      ...(opts.onOpen ? [{ label: "開く", onClick: opts.onOpen }] : []),
      { label: "詳細情報…", onClick: () => setInfo(photo) },
    ]);

    groups.push([
      {
        label: fav ? "お気に入りから外す" : "お気に入りに追加",
        onClick: () =>
          run(async () => {
            await setPhotoFavorite(id, !fav);
            setFavOv((o) => new Map(o).set(id, !fav));
            opts.onFavoriteChanged?.(id, !fav);
          }),
      },
      { label: "アルバムに追加…", onClick: () => void addToAlbum([id]) },
      ...(opts.extra ?? []),
    ]);

    groups.push([
      { label: "画像をコピー", disabled: photo.missing || photo.onlineOnly, onClick: () => run(() => copyPhotoImage(id)) },
      { label: "フルパスをコピー", onClick: () => run(() => copyText(photo.path)) },
      { label: "ファイル名をコピー", onClick: () => run(() => copyText(photo.fileName)) },
    ]);

    groups.push([
      { label: "フォルダで表示", disabled: photo.missing, onClick: () => run(() => revealPhoto(id)) },
      { label: "既定のアプリで開く", disabled: photo.missing, onClick: () => run(() => openPhoto(id)) },
    ]);

    const jump: MenuItem[] = [];
    if (photo.visitId != null) {
      const v = photo.visitId;
      jump.push({ label: "このセッションを表示", onClick: () => nav.showVisit(v) });
    }
    jump.push({ label: "この日のタイムライン", onClick: () => nav.showDayAt(photo.takenAt) });
    if (photo.worldRefId != null) {
      const w = photo.worldRefId;
      jump.push({ label: "ワールドを表示", onClick: () => nav.showWorld(w) });
    }
    const wid = detail?.worldId;
    if (wid && wid.startsWith("wrld_")) {
      jump.push({ label: "VRChat のワールドページを開く", onClick: () => run(() => openWorldPage(wid)) });
    }
    groups.push(jump);

    const md: MenuItem[] = [];
    if (status === "loading") {
      md.push({ label: "メタデータを確認中…", disabled: true, onClick: () => {} });
    } else if (status === "failed") {
      md.push({ label: "メタデータの状態を取得できませんでした", disabled: true, onClick: () => {} });
    } else {
      const base = { target: { kind: "ids" as const, ids: [id] }, label: photo.fileName, allowDownload: status.writableAfterDownload };
      if (status.writable || status.writableAfterDownload) {
        md.push({ label: "メタデータを書き込む…", onClick: () => metadata.open(base) });
      }
      if (status.removable) {
        md.push({ label: "書き込んだメタデータを取り除く…", onClick: () => metadata.open({ ...base, mode: "remove" }) });
      }
      if (md.length === 0) {
        const why = status.reason ? SKIP_LABELS[status.reason] : "";
        md.push({ label: `メタデータを書き込めません${why ? `（${why}）` : ""}`, disabled: true, onClick: () => {} });
      }
    }
    groups.push(md);

    const out: MenuEntry[] = [];
    for (const g of groups) {
      if (g.length === 0) continue;
      if (out.length > 0) out.push("separator");
      out.push(...g);
    }
    return out;
  };

  const element = (
    <>
      {menu && <ContextMenu x={menu.x} y={menu.y} items={build(menu)} onClose={closeMenu} />}
      {info && <PhotoInfoDialog photo={info} onClose={() => setInfo(null)} />}
    </>
  );
  return { onContextMenu, element };
}
