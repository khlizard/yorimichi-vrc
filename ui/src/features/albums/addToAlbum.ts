import { useCallback } from "react";
import { albumAddPhotos, albums, createAlbum } from "../../libraryApi";
import { useDialog } from "../../components/Dialog";
import { errText } from "../../components/util";
import { nf } from "../../format";

export interface AddToAlbumResult {
  albumId: number;
  albumName: string;
  /** Newly added photos. */
  added: number;
  /** Photos that were already in the album. */
  already: number;
}

const NEW_ALBUM = "new";

/** "3 枚を「旅行」に追加しました（1 枚は追加済み）" */
export function describeAdd(r: AddToAlbumResult): string {
  const base = `${nf(r.added)} 枚を「${r.albumName}」に追加しました`;
  return r.already > 0 ? `${base}（${nf(r.already)} 枚は追加済み）` : base;
}

/**
 * Returns a function that lets the user pick (or create) an album and adds the photos to it.
 * Resolves null when cancelled or on error (the error is shown in a dialog).
 */
export function useAddToAlbum(): (photoIds: number[]) => Promise<AddToAlbumResult | null> {
  const { pick, prompt, showError } = useDialog();
  return useCallback(
    async (photoIds: number[]) => {
      if (photoIds.length === 0) return null;
      try {
        const list = await albums();
        const choice = await pick<number>({
          title: "アルバムに追加",
          items: list.map((a) => ({ id: a.id, label: a.name, hint: `${nf(a.photoCount)} 枚` })),
          emptyText: "アルバムはまだありません。",
          extra: { id: NEW_ALBUM, label: "新しいアルバムを作る…" },
        });
        if (choice === null) return null;
        let albumId: number;
        let albumName: string;
        if (choice === NEW_ALBUM) {
          const name = await prompt({ title: "新しいアルバム", label: "アルバムの名前", okLabel: "作成" });
          if (name === null) return null;
          const created = await createAlbum(name, "");
          albumId = created.id;
          albumName = created.name;
        } else {
          albumId = choice as number;
          albumName = list.find((a) => a.id === albumId)?.name ?? "";
        }
        const added = await albumAddPhotos(albumId, photoIds);
        return { albumId, albumName, added, already: Math.max(0, photoIds.length - added) };
      } catch (e) {
        showError(errText(e));
        return null;
      }
    },
    [pick, prompt, showError],
  );
}
