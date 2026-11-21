import { useEffect, useImperativeHandle, useMemo, useRef, useState, forwardRef } from "react";
import { onPhotosChanged, type PhotoRow } from "../api";
import { useDialog } from "../components/Dialog";
import { useMetadataDialog } from "../metadata/MetadataProvider";
import { useLatest } from "../components/hooks";
import { errText, type TabHandle } from "../components/util";
import { useNav } from "../nav";
import { Lightbox } from "../photos/Lightbox";
import { PhotoGrid } from "../photos/PhotoGrid";
import { Thumb } from "../photos/Thumb";
import {
  albumPhotos,
  albumRemovePhotos,
  albumReorder,
  albums as fetchAlbums,
  createAlbum,
  deleteAlbum,
  exportAlbumHtml,
  updateAlbum,
  type AlbumRow,
} from "../libraryApi";
import "../photos/photos.css";
import "./albums.css";

export interface AlbumsHandle extends TabHandle {
  selectAlbum(albumId: number): void;
}

interface Props {
  active: boolean;
  dayStartHour: number;
}

const dateOf = (t: string | null) => (t ? t.slice(0, 10) : "");
const noop = () => {};

function rangeText(a: AlbumRow): string {
  const n = `${a.photoCount.toLocaleString("ja-JP")} 枚`;
  const f = dateOf(a.firstTaken);
  const l = dateOf(a.lastTaken);
  return f ? `${n} · ${f} 〜 ${l}` : n;
}

function Cover({ id }: { id: number | null }) {
  return (
    <span className="al-cover" aria-hidden="true">
      {id !== null && <Thumb photo={{ id, onlineOnly: false }} />}
    </span>
  );
}

export const AlbumsTab = forwardRef<AlbumsHandle, Props>(function AlbumsTab({ active, dayStartHour }, ref) {
  const { showError, choose, prompt } = useDialog();
  const metadata = useMetadataDialog();
  const nav = useNav();
  const activated = useRef(false);
  const gen = useRef(0);
  const [list, setList] = useState<AlbumRow[]>([]);
  const [loaded, setLoaded] = useState(false);
  const [selectedId, setSelectedId] = useState<number | null>(null);
  const selectedRef = useRef<number | null>(null);
  const [photos, setPhotos] = useState<PhotoRow[]>([]);
  const [name, setName] = useState("");
  const [desc, setDesc] = useState("");
  /** Name / description as last saved, to detect edits. */
  const saved = useRef({ id: -1, name: "", desc: "" });
  const draft = useLatest({ name, desc });
  const [selecting, setSelecting] = useState(false);
  const [sel, setSel] = useState<Set<number>>(() => new Set());
  const [lb, setLb] = useState<number | null>(null);
  const [busy, setBusy] = useState(false);

  const current = useMemo(() => list.find((a) => a.id === selectedId) ?? null, [list, selectedId]);
  const dirty = current !== null && (name !== saved.current.name || desc !== saved.current.desc);

  /** Load the album list and the photos of the (kept or given) selected album. */
  const load = async (want?: number | null) => {
    const g = ++gen.current;
    try {
      const rows = await fetchAlbums();
      if (g !== gen.current) return;
      const id = rows.find((a) => a.id === (want ?? selectedRef.current))?.id ?? rows[0]?.id ?? null;
      const row = rows.find((a) => a.id === id) ?? null;
      const ph = id === null ? [] : await albumPhotos(id);
      if (g !== gen.current) return;
      const same = selectedRef.current === id;
      const wasDirty = same && (saved.current.name !== draft.current.name || saved.current.desc !== draft.current.desc);
      selectedRef.current = id;
      setList(rows);
      setSelectedId(id);
      setPhotos(ph);
      setLoaded(true);
      if (row) {
        saved.current = { id: row.id, name: row.name, desc: row.description };
        if (!wasDirty) {
          setName(row.name);
          setDesc(row.description);
        }
      }
      if (!same) {
        setSel(new Set());
        setLb(null);
      } else {
        const ids = new Set(ph.map((p) => p.id));
        setSel((s) => new Set([...s].filter((x) => ids.has(x))));
      }
    } catch (e) {
      if (g === gen.current) showError(errText(e));
    }
  };

  useEffect(() => {
    if (active && !activated.current) {
      activated.current = true;
      void load();
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [active]);

  useEffect(() => {
    const un = onPhotosChanged(() => {
      if (activated.current) void load();
    });
    return () => {
      un.then((f) => f()).catch(noop);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useImperativeHandle(ref, () => ({
    refresh: () => {
      if (activated.current) void load();
    },
    selectAlbum: (id: number) => {
      activated.current = true;
      void load(id);
    },
  }));

  const pickAlbum = (a: AlbumRow) => {
    if (a.id === selectedRef.current) return;
    // Switching drops unsaved edits of the previous album.
    void load(a.id);
  };

  const guard = async (fn: () => Promise<void>) => {
    if (busy) return;
    setBusy(true);
    try {
      await fn();
    } catch (e) {
      showError(errText(e));
    } finally {
      setBusy(false);
    }
  };

  const newAlbum = async () => {
    const n = await prompt({ title: "新しいアルバム", label: "アルバムの名前", okLabel: "作成" });
    if (n === null) return;
    await guard(async () => {
      const a = await createAlbum(n, "");
      await load(a.id);
    });
  };

  const save = () =>
    guard(async () => {
      if (!current || name.trim() === "") return;
      await updateAlbum(current.id, name.trim(), desc, current.coverPhotoId);
      await load();
      setName(name.trim());
    });

  const removeIds = async (ids: number[]) => {
    if (!current || ids.length === 0) return false;
    const r = await choose({
      title: "アルバムから外す",
      message: `選んだ ${ids.length.toLocaleString("ja-JP")} 枚をアルバムから外します。写真のファイルは消えません。`,
      buttons: [
        { id: "ok", label: "外す", primary: true },
        { id: "cancel", label: "キャンセル" },
      ],
      cancelId: "cancel",
    });
    if (r !== "ok") return false;
    await guard(async () => {
      await albumRemovePhotos(current.id, ids);
      await load();
    });
    return true;
  };

  const reorder = (toFront: boolean) =>
    guard(async () => {
      if (!current || sel.size === 0) return;
      const picked = photos.filter((p) => sel.has(p.id)).map((p) => p.id);
      const rest = photos.filter((p) => !sel.has(p.id)).map((p) => p.id);
      await albumReorder(current.id, toFront ? [...picked, ...rest] : [...rest, ...picked]);
      await load();
    });

  const setCover = () =>
    guard(async () => {
      if (!current || sel.size !== 1) return;
      await updateAlbum(current.id, current.name, current.description, [...sel][0]);
      await load();
    });

  const exportHtml = async () => {
    if (!current) return;
    const r = await choose({
      title: "HTML に書き出す",
      message: "会った人の名前を含めますか？",
      buttons: [
        { id: "yes", label: "含める", primary: true },
        { id: "no", label: "含めない" },
        { id: "cancel", label: "キャンセル" },
      ],
      cancelId: "cancel",
    });
    if (r !== "yes" && r !== "no") return;
    await guard(() => exportAlbumHtml(current.id, r === "yes"));
  };

  const remove = async () => {
    if (!current) return;
    const r = await choose({
      title: "アルバムを削除",
      message: `アルバム「${current.name}」を削除します。写真のファイルは消えません。`,
      buttons: [
        { id: "ok", label: "削除", primary: true },
        { id: "cancel", label: "キャンセル" },
      ],
      cancelId: "cancel",
    });
    if (r !== "ok") return;
    await guard(async () => {
      await deleteAlbum(current.id);
      selectedRef.current = null;
      await load();
    });
  };

  const toggleSelecting = () => {
    setSelecting((s) => !s);
    setSel(new Set());
  };

  const emptyAlbums = loaded && list.length === 0;

  return (
    <div className="photo-surface al">
      <div className="al-list-pane">
        <div className="al-list-head">
          <button type="button" onClick={() => void newAlbum()} disabled={busy}>
            新しいアルバム
          </button>
        </div>
        {emptyAlbums ? (
          <div className="al-empty">アルバムはまだありません。写真タブで写真を選んで追加できます。</div>
        ) : (
          <ul className="al-list" aria-label="アルバム一覧">
            {list.map((a) => (
              <li key={a.id}>
                <button
                  type="button"
                  className="al-item"
                  aria-current={a.id === selectedId ? "true" : undefined}
                  onClick={() => pickAlbum(a)}
                >
                  <Cover id={a.coverPhotoId} />
                  <span className="al-item-text">
                    <span className="al-item-name ellipsis">{a.name}</span>
                    <span className="al-item-meta ellipsis">{rangeText(a)}</span>
                  </span>
                </button>
              </li>
            ))}
          </ul>
        )}
      </div>
      <div className="al-detail">
        {current ? (
          <>
            <div className="al-edit">
              <label className="al-field">
                <span>名前</span>
                <input type="text" value={name} onChange={(e) => setName(e.target.value)} />
              </label>
              <label className="al-field">
                <span>説明</span>
                <textarea rows={2} value={desc} onChange={(e) => setDesc(e.target.value)} />
              </label>
              <div className="al-edit-actions">
                <button type="button" disabled={!dirty || name.trim() === "" || busy} onClick={() => void save()}>
                  保存
                </button>
              </div>
            </div>
            <div className="ps-toolbar" role="toolbar" aria-label="アルバムの操作">
              <button type="button" aria-pressed={selecting} onClick={toggleSelecting}>
                選択
              </button>
              {selecting && (
                <>
                  <span className="ps-total" role="status">
                    {sel.size.toLocaleString("ja-JP")} 枚を選択中
                  </span>
                  <button type="button" disabled={sel.size === 0 || busy} onClick={() => void removeIds([...sel])}>
                    アルバムから外す
                  </button>
                  <button type="button" disabled={sel.size !== 1 || busy} onClick={() => void setCover()}>
                    表紙にする
                  </button>
                  <button type="button" disabled={sel.size === 0 || busy} onClick={() => void reorder(true)}>
                    先頭へ移動
                  </button>
                  <button type="button" disabled={sel.size === 0 || busy} onClick={() => void reorder(false)}>
                    末尾へ移動
                  </button>
                </>
              )}
              <span className="ps-spacer" />
              <button type="button" disabled={busy} onClick={() => void exportHtml()}>
                HTML に書き出す…
              </button>
              <button
                type="button"
                disabled={busy || !current}
                title="このアルバムの写真にメタデータを書き込む"
                onClick={() =>
                  current && metadata.open({ target: { kind: "album", albumId: current.id }, label: `アルバム「${current.name}」` })
                }
              >
                メタデータ…
              </button>
              <button type="button" disabled={busy} onClick={() => void remove()}>
                アルバムを削除
              </button>
            </div>
            <PhotoGrid
              items={photos}
              dayStartHour={dayStartHour}
              groupByDay={false}
              size="m"
              active={active}
              onOpen={(i) => setLb(i)}
              selectable={selecting}
              selectedIds={sel}
              onToggleSelect={(id, range) =>
                setSel((s) => {
                  const n = new Set(s);
                  if (range && range.length > 0) range.forEach((x) => n.add(x));
                  else if (n.has(id)) n.delete(id);
                  else n.add(id);
                  return n;
                })
              }
              emptyText="このアルバムにはまだ写真がありません。写真タブで写真を選んで追加できます。"
            />
          </>
        ) : (
          <div className="al-empty">{loaded ? "アルバムを選んでください。" : "読み込み中…"}</div>
        )}
      </div>
      {lb !== null && (
        <Lightbox
          items={photos}
          index={lb}
          onIndexChange={setLb}
          onClose={() => setLb(null)}
          onNavigateVisit={(id) => {
            setLb(null);
            nav.showVisit(id);
          }}
          onNavigatePlayer={(id, n) => {
            setLb(null);
            nav.showPlayer(id, n);
          }}
          onFavoriteChanged={(id, fav) => setPhotos((l) => l.map((x) => (x.id === id ? { ...x, favorite: fav } : x)))}
          extraActions={[{ label: "このアルバムから外す", onClick: (p) => void removeIds([p.id]) }]}
        />
      )}
    </div>
  );
});
