import { useRef } from "react";

/** Always-current value for use inside async callbacks (updated during render). */
export function useLatest<T>(value: T) {
  const ref = useRef(value);
  ref.current = value;
  return ref;
}
