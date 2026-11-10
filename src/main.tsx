import React from "react";
import ReactDOM from "react-dom/client";
import "./styles.css";
import App from "./App";
import { SettingsApp } from "./features/settings/SettingsApp";
import { SearchBarApp } from "./features/search/SearchBarApp";
import { ThemeRoot } from "./themes/ThemeRoot";
import { initPlatform, PLATFORM } from "./lib/platform";

// Platform-specific CSS (e.g. no traffic-light spacing under the native Windows title bar).
document.documentElement.dataset.platform = PLATFORM;

// The Settings window and the floating search bar load the same bundle at
// `index.html#settings` and `index.html#search`.
const route = window.location.hash;
if (route === "#search") document.documentElement.dataset.window = "search";
const Root = route === "#settings" ? SettingsApp : route === "#search" ? SearchBarApp : App;

// Windows 11 detection is async (Client Hints) and decides whether translucency can turn on.
void initPlatform().finally(() => {
  ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
    <React.StrictMode>
      <ThemeRoot>
        <Root />
      </ThemeRoot>
    </React.StrictMode>,
  );
});
