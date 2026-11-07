import React from "react";
import ReactDOM from "react-dom/client";
import "./styles.css";
import App from "./App";
import { SettingsApp } from "./features/settings/SettingsApp";
import { ThemeRoot } from "./themes/ThemeRoot";

// The Settings window loads the same bundle at `index.html#settings`.
const isSettingsWindow = window.location.hash === "#settings";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <ThemeRoot>{isSettingsWindow ? <SettingsApp /> : <App />}</ThemeRoot>
  </React.StrictMode>,
);
