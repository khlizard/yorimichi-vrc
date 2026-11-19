import "./star.css";

const PATH = "M12 2l3.1 6.6 7.2.9-5.3 5 1.4 7.1L12 18l-6.4 3.6 1.4-7.1-5.3-5 7.2-.9z";

/** Favourite star: solid when filled, outline otherwise. Colour comes from --star / --muted. */
export function StarIcon({ filled, size = 14 }: { filled: boolean; size?: number }) {
  return (
    <svg className={`star-icon${filled ? " on" : ""}`} width={size} height={size} viewBox="0 0 24 24" aria-hidden="true">
      <path
        d={PATH}
        fill={filled ? "currentColor" : "none"}
        stroke={filled ? "none" : "currentColor"}
        strokeWidth={filled ? 0 : 1.5 * (24 / size)}
        strokeLinejoin="round"
      />
    </svg>
  );
}

export function StarButton({
  active,
  onToggle,
  label,
  disabled,
}: {
  active: boolean;
  onToggle: () => void;
  /** Overrides the default aria-label / title. */
  label?: string;
  disabled?: boolean;
}) {
  const text = label ?? (active ? "お気に入りを外す" : "お気に入りにする");
  return (
    <button type="button" className="star-btn" aria-pressed={active} aria-label={text} title={text} disabled={disabled} onClick={onToggle}>
      <StarIcon filled={active} size={18} />
    </button>
  );
}
