import { useEffect, useRef, useState } from "react";
import { useDialog } from "../../components/Dialog";
import { errText } from "../../components/util";
import {
  createTag,
  deleteTag,
  openUserPage,
  setPlayerMemo,
  setPlayerTags,
  updateTag,
  type PlayerProfile,
  type TagCount,
} from "../../libraryApi";
import { useNav } from "../../nav";
import { Lightbox } from "../photos/Lightbox";
import { PhotoStrip, useRecentPhotos } from "../photos/PhotoStrip";
import { TAG_COLORS, TagChip, TagDot } from "./tagUi";
import { fmtDur, nf } from "../../format";
import { withFavorite } from "../photos/usePhotoSelection";
import "./players.css";

/** Memos are one line; older multi-line memos are shown joined by a space. */
const oneLine = (s: string) => s.replace(/\s*[\r\n]+\s*/g, " ");

const dateOf = (t: string | null) => (t ? t.slice(0, 10) : "");

export function factsLine(p: PlayerProfile): string {
  const parts: string[] = [];
  if (p.firstMet) parts.push(`初めて会った日 ${dateOf(p.firstMet)}`);
  if (p.lastMet) parts.push(`最後 ${dateOf(p.lastMet)}`);
  parts.push(`一緒にいた時間 ${fmtDur(p.totalSeconds)}`);
  parts.push(`${nf(p.encounterCount)} 回`);
  parts.push(`写真 ${nf(p.photoCount)} 枚`);
  return parts.join(" · ");
}

interface Props {
  profile: PlayerProfile;
  allTags: TagCount[];
  /** Called after memo / tag changes so the owner can refresh marks, counts and the profile. */
  onChanged: () => void;
}

export function PlayerProfilePanel({ profile, allTags, onChanged }: Props) {
  const { showError, choose, prompt } = useDialog();
  const nav = useNav();
  const [memo, setMemo] = useState(() => oneLine(profile.memo));
  const [savedMemo, setSavedMemo] = useState(() => oneLine(profile.memo));
  const [lbIndex, setLbIndex] = useState<number | null>(null);
  const [recent, setRecent] = useRecentPhotos({ playerId: profile.id });
  const [memoNote, setMemoNote] = useState(false);
  const [editing, setEditing] = useState(false);
  const [newName, setNewName] = useState("");
  const [newColor, setNewColor] = useState(TAG_COLORS[5].hex);
  const [busy, setBusy] = useState(false);
  const editBtn = useRef<HTMLButtonElement>(null);

  // A different player replaces the drafts.
  useEffect(() => {
    setMemo(oneLine(profile.memo));
    setSavedMemo(oneLine(profile.memo));
    setMemoNote(false);
    setLbIndex(null);
    setEditing(false);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [profile.id]);

  const run = async (fn: () => Promise<void>) => {
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

  const saveMemo = () =>
    run(async () => {
      await setPlayerMemo(profile.id, memo);
      setSavedMemo(memo);
      setMemoNote(true);
      onChanged();
    });

  const owned = new Set(profile.tags.map((t) => t.id));
  const toggleTag = (id: number) =>
    run(async () => {
      const next = new Set(owned);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      await setPlayerTags(profile.id, [...next]);
      onChanged();
    });

  const addTag = () =>
    run(async () => {
      const name = newName.trim();
      if (name === "") return;
      const t = await createTag(name, newColor);
      await setPlayerTags(profile.id, [...owned, t.id]);
      setNewName("");
      onChanged();
    });

  const renameTag = async (t: TagCount) => {
    const name = await prompt({ title: "タグの名前を変更", label: "タグの名前", initial: t.name, okLabel: "変更" });
    if (name === null || name === t.name) return;
    await run(async () => {
      await updateTag(t.id, name, t.color);
      onChanged();
    });
  };

  const removeTag = async (t: TagCount) => {
    const r = await choose({
      title: "タグを削除",
      message: `タグ「${t.name}」を削除します。${nf(t.playerCount)} 人から外れます。`,
      buttons: [
        { id: "ok", label: "削除", primary: true },
        { id: "cancel", label: "キャンセル" },
      ],
      cancelId: "cancel",
    });
    if (r !== "ok") return;
    await run(async () => {
      await deleteTag(t.id);
      onChanged();
    });
  };

  return (
    <section className="pp" aria-label="プレイヤーの情報">
      <div className="pp-head">
        <h2 className="pp-name">{profile.displayName}</h2>
        <button
          type="button"
          onClick={() => openUserPage(profile.userId).catch((e) => showError(errText(e)))}
          disabled={profile.userId === ""}
        >
          VRChat のユーザーページ
        </button>
      </div>
      <div className="pp-facts">{factsLine(profile)}</div>

      <div className="pp-row">
        <label className="pp-label" htmlFor="pp-memo">
          メモ
        </label>
        <div className="pp-memo">
          <input
            id="pp-memo"
            type="text"
            value={memo}
            onChange={(e) => {
              setMemo(e.target.value);
              setMemoNote(false);
            }}
            onKeyDown={(e) => {
              if (e.key === "Enter" && !e.nativeEvent.isComposing) {
                e.preventDefault();
                if (memo !== savedMemo && !busy) void saveMemo();
              }
            }}
          />
          <button type="button" disabled={memo === savedMemo || busy} onClick={() => void saveMemo()}>
            保存
          </button>
          <span className="save-status" role="status">
            {memoNote && memo === savedMemo ? "保存しました" : ""}
          </span>
        </div>
      </div>

      <div className="pp-row">
        <span className="pp-label">タグ</span>
        <div className="pp-tags">
          <div className="pp-chips">
            {profile.tags.length === 0 && <span className="muted">タグはありません</span>}
            {profile.tags.map((t) => (
              <TagChip key={t.id} tag={t} />
            ))}
            <button
              ref={editBtn}
              type="button"
              aria-expanded={editing}
              aria-controls="pp-tag-editor"
              onClick={() => setEditing((v) => !v)}
            >
              タグを編集
            </button>
          </div>
          {editing && (
            <div
              id="pp-tag-editor"
              className="tag-editor"
              role="group"
              aria-label="タグの編集"
              onKeyDown={(e) => {
                if (e.key === "Escape") {
                  e.stopPropagation();
                  setEditing(false);
                  editBtn.current?.focus();
                }
              }}
            >
              {allTags.length === 0 && <div className="muted">タグはまだありません。下で作成できます。</div>}
              <ul className="tag-list">
                {allTags.map((t) => (
                  <li key={t.id}>
                    <label className="tag-check">
                      <input type="checkbox" checked={owned.has(t.id)} disabled={busy} onChange={() => void toggleTag(t.id)} />
                      <TagDot color={t.color} />
                      <span>{t.name}</span>
                      <span className="faint small">({t.playerCount})</span>
                    </label>
                    <button type="button" disabled={busy} onClick={() => void renameTag(t)} aria-label={`タグ「${t.name}」の名前を変更`}>
                      名前を変更
                    </button>
                    <button type="button" disabled={busy} onClick={() => void removeTag(t)} aria-label={`タグ「${t.name}」を削除`}>
                      削除
                    </button>
                  </li>
                ))}
              </ul>
              <div className="tag-new">
                <label htmlFor="pp-newtag">新しいタグ</label>
                <input
                  id="pp-newtag"
                  type="text"
                  value={newName}
                  maxLength={40}
                  onChange={(e) => setNewName(e.target.value)}
                  onKeyDown={(e) => {
                    if (e.key === "Enter" && !e.nativeEvent.isComposing) {
                      e.preventDefault();
                      void addTag();
                    }
                  }}
                />
                <div className="tag-swatches" role="radiogroup" aria-label="タグの色">
                  {TAG_COLORS.map((c) => (
                    <button
                      key={c.hex}
                      type="button"
                      role="radio"
                      aria-checked={newColor === c.hex}
                      aria-label={c.name}
                      title={c.name}
                      className={`tag-swatch${newColor === c.hex ? " on" : ""}`}
                      style={{ background: c.hex }}
                      onClick={() => setNewColor(c.hex)}
                    />
                  ))}
                </div>
                <button type="button" disabled={newName.trim() === "" || busy} onClick={() => void addTag()}>
                  追加してこの人に付ける
                </button>
              </div>
            </div>
          )}
        </div>
      </div>
      {recent.length > 0 && (
        <PhotoStrip
          clip
          title="一緒にいたときの写真"
          photos={recent}
          onOpen={setLbIndex}
          onFavoriteChanged={(id, fav) => setRecent((l) => withFavorite(l, id, fav))}
          actions={
            <button type="button" onClick={() => nav.showPhotos({ playerId: profile.id, label: profile.displayName })}>
              すべて見る ({nf(profile.photoCount)})
            </button>
          }
        />
      )}
      {lbIndex !== null && (
        <Lightbox
          items={recent}
          index={lbIndex}
          onIndexChange={setLbIndex}
          onClose={() => setLbIndex(null)}
          onFavoriteChanged={(id, fav) => setRecent((l) => withFavorite(l, id, fav))}
        />
      )}
    </section>
  );
}
