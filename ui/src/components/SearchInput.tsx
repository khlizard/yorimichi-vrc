import { forwardRef, useImperativeHandle, useRef, type InputHTMLAttributes } from "react";

type Props = Omit<InputHTMLAttributes<HTMLInputElement>, "type" | "value"> & {
  value: string;
  onClear(): void;
};

/** Text input with a faint clear (x) button inside the right edge. */
export const SearchInput = forwardRef<HTMLInputElement, Props>(function SearchInput(
  { className, onClear, onKeyDown, value, ...rest },
  ref,
) {
  const inner = useRef<HTMLInputElement>(null);
  useImperativeHandle(ref, () => inner.current as HTMLInputElement);

  return (
    <span className={className ? `search-input ${className}` : "search-input"}>
      <input
        {...rest}
        ref={inner}
        type="text"
        value={value}
        onKeyDown={(e) => {
          onKeyDown?.(e);
          if (e.key === "Escape" && value !== "") {
            e.stopPropagation();
            onClear();
          }
        }}
      />
      {value !== "" && (
        <button
          type="button"
          className="search-clear"
          aria-label="検索をクリア"
          title="クリア"
          onClick={() => {
            onClear();
            inner.current?.focus();
          }}
        >
          <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true">
            <path d="M1 1L9 9M9 1L1 9" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" fill="none" />
          </svg>
        </button>
      )}
    </span>
  );
});
