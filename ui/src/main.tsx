import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import SettingsWindow from "./SettingsWindow";
import "./styles.css";
import { initTheme } from "./theme";

initTheme();

const isSettings = new URLSearchParams(location.search).get("window") === "settings";

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>{isSettings ? <SettingsWindow /> : <App />}</React.StrictMode>,
);
