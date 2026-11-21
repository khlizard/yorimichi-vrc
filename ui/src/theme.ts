import { getSettings, onSettingsChanged, type Settings } from "./api";

export type ThemeSetting = "auto" | "light" | "dark";

const STORAGE_KEY = "vrclogger.theme";
const query = () => window.matchMedia("(prefers-color-scheme: dark)");

export function normalizeTheme(v: unknown): ThemeSetting {
  return v === "light" || v === "dark" ? v : "auto";
}

let current: ThemeSetting = "auto";
let listening = false;

function onOsChange() {
  if (current === "auto") render();
}

function render() {
  const dark = current === "dark" || (current === "auto" && query().matches);
  document.documentElement.dataset.theme = dark ? "dark" : "light";
}

/** Applies a theme to this window. Auto follows the OS and reacts live (one shared listener). */
export function applyTheme(setting: ThemeSetting) {
  current = normalizeTheme(setting);
  if (!listening) {
    listening = true;
    query().addEventListener("change", onOsChange);
  }
  render();
}

function applyAndStore(setting: ThemeSetting) {
  applyTheme(setting);
  try {
    localStorage.setItem(STORAGE_KEY, current);
  } catch {
    /* storage unavailable: the theme is still applied for this session */
  }
}

/** Call before React renders: last-known theme first (no flash), then the saved setting. */
export function initTheme() {
  let last: ThemeSetting = "auto";
  try {
    last = normalizeTheme(localStorage.getItem(STORAGE_KEY));
  } catch {
    /* ignore */
  }
  applyTheme(last);
  getSettings()
    .then((s: Settings) => applyAndStore(normalizeTheme(s.Theme)))
    .catch(() => {});
  void onSettingsChanged((s) => applyAndStore(normalizeTheme(s.Theme)));
}
