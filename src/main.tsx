import React from "react";
import ReactDOM from "react-dom/client";
import "./styles.css";
import App from "./App";
import { SettingsApp } from "./features/settings/SettingsApp";
import { SearchBarApp } from "./features/search/SearchBarApp";
import { ThemeRoot } from "./themes/ThemeRoot";

// The Settings window and the floating search bar load the same bundle at
// `index.html#settings` and `index.html#search`.
const route = window.location.hash;
if (route === "#search") document.documentElement.dataset.window = "search";
const Root = route === "#settings" ? SettingsApp : route === "#search" ? SearchBarApp : App;

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <ThemeRoot>
      <Root />
    </ThemeRoot>
  </React.StrictMode>,
);
