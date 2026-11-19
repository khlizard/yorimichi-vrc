import type { Tag } from "../libraryApi";
import "./players.css";

/** Preset tag colours (stored as hex). Muted, distinguishable from each other; used only for the dot. */
export const TAG_COLORS: { hex: string; name: string }[] = [
  { hex: "#c8453f", name: "赤" },
  { hex: "#d9822b", name: "オレンジ" },
  { hex: "#a58a00", name: "黄土" },
  { hex: "#2f8f4e", name: "緑" },
  { hex: "#1f8f95", name: "青緑" },
  { hex: "#2b6fd6", name: "青" },
  { hex: "#8a6a4a", name: "茶" },
  { hex: "#6b7280", name: "灰" },
];

export function TagDot({ color }: { color: string }) {
  return <span className="tag-dot" style={{ background: color }} aria-hidden="true" />;
}

/** Chip: coloured dot + name in the normal text colour (the tag colour never carries the text). */
export function TagChip({ tag }: { tag: Tag }) {
  return (
    <span className="tag-chip">
      <TagDot color={tag.color} />
      {tag.name}
    </span>
  );
}

/** Compact dots (and a memo marker) for list rows. */
export function RowMarks({ tags, hasMemo }: { tags: Tag[]; hasMemo: boolean }) {
  if (tags.length === 0 && !hasMemo) return null;
  const shown = tags.slice(0, 5);
  const label = [tags.length > 0 ? `タグ: ${tags.map((t) => t.name).join("、")}` : "", hasMemo ? "メモあり" : ""]
    .filter(Boolean)
    .join(" / ");
  return (
    <span className="row-marks" title={label}>
      {shown.map((t) => (
        <TagDot key={t.id} color={t.color} />
      ))}
      {tags.length > shown.length && <span className="faint small">+{tags.length - shown.length}</span>}
      {hasMemo && (
        <svg className="memo-mark" viewBox="0 0 12 12" width="10" height="10" aria-hidden="true">
          <path d="M2 1h8v10H2z M4 4h4 M4 6.5h4 M4 9h2" fill="none" stroke="currentColor" strokeWidth="1" />
        </svg>
      )}
      <span className="sr-only">{label}</span>
    </span>
  );
}
