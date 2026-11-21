import { useEffect, useState } from "react";
import { thumbUrl, type PhotoRow } from "../../api";
import "./photos.css";

/** Thumbnail that fills its parent box; shows a quiet placeholder when the image cannot be loaded. */
export function Thumb({ photo, className }: { photo: Pick<PhotoRow, "id" | "onlineOnly">; className?: string }) {
  const [bad, setBad] = useState(false);
  useEffect(() => setBad(false), [photo.id]);
  return (
    <span className={`thumb${className ? ` ${className}` : ""}`}>
      {bad ? (
        <span className="thumb-missing">{photo.onlineOnly ? "未ダウンロード" : "表示できません"}</span>
      ) : (
        <img loading="lazy" decoding="async" src={thumbUrl(photo.id)} alt="" draggable={false} onError={() => setBad(true)} />
      )}
    </span>
  );
}
