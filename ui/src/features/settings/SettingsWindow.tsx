import { getCurrentWindow } from "@tauri-apps/api/window";
import { useEffect } from "react";
import { DialogProvider } from "../../components/Dialog";
import { SettingsTab } from "./SettingsTab";

export default function SettingsWindow() {
  useEffect(() => {
    document.title = "設定 — Yorimichi";
  }, []);
  return (
    <DialogProvider>
      <SettingsTab onClose={() => void getCurrentWindow().close()} />
    </DialogProvider>
  );
}
