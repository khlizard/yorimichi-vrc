import { createContext, useCallback, useContext, useMemo, useState, type ReactNode } from "react";
import type { MetaTarget } from "../metadataApi";
import { MetadataDialog } from "./MetadataDialog";

export interface MetadataOpenOptions {
  /** Omit to let the user choose a date range (File menu). */
  target?: MetaTarget;
  label?: string;
}

interface MetadataApi {
  open(opts: MetadataOpenOptions): void;
}

const Ctx = createContext<MetadataApi>({ open: () => {} });

export function useMetadataDialog(): MetadataApi {
  return useContext(Ctx);
}

export function MetadataProvider({ children }: { children: ReactNode }) {
  const [state, setState] = useState<{ opts: MetadataOpenOptions; n: number } | null>(null);
  // Ignore opens while one is already showing.
  const open = useCallback((opts: MetadataOpenOptions) => setState((p) => p ?? { opts, n: Date.now() }), []);
  const api = useMemo(() => ({ open }), [open]);
  return (
    <Ctx.Provider value={api}>
      {children}
      {state && <MetadataDialog key={state.n} opts={state.opts} onClose={() => setState(null)} />}
    </Ctx.Provider>
  );
}
