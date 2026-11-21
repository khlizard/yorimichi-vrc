import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import SettingsWindow from "./features/settings/SettingsWindow";
import "./styles.css";
import { initTheme } from "./theme";

async function start() {
  // Dev server only: `?demo` runs the UI on dummy data in a plain browser (for screenshots).
  if (import.meta.env.DEV && new URLSearchParams(location.search).has("demo")) {
    await import("./demo/installDemo");
  }

  initTheme();

  const isSettings = new URLSearchParams(location.search).get("window") === "settings";

  ReactDOM.createRoot(document.getElementById("root")!).render(
    <React.StrictMode>{isSettings ? <SettingsWindow /> : <App />}</React.StrictMode>,
  );
}

void start();
