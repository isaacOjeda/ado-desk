// Imported themes (Open VSX). Search, download and mapping run in Rust (src-tauri/src/themes);
// the webview only receives validated token maps.
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { call } from "../lib/api";
import type { Theme } from "./types";

/** An Open VSX extension in the Themes category. */
export interface ThemeExtension {
  namespace: string;
  name: string;
  displayName: string;
  description: string;
  version: string;
  downloadCount: number;
  verified: boolean;
}

/** Emitted by Rust to every window with the full imported list after an import or removal. */
export const THEMES_CHANGED_EVENT = "themes-changed";

export const themesApi = {
  /** Themes on Open VSX matching `query`; an empty query lists the most downloaded ones. */
  search: (query: string) => call<ThemeExtension[]>("search_theme_extensions", { query }),
  /** Imports every color theme of the extension; resolves with the themes just imported. */
  importExtension: (namespace: string, name: string) => call<Theme[]>("import_theme_extension", { namespace, name }),
  list: () => call<Theme[]>("list_imported_themes"),
  /** Removes one imported theme; resolves with the remaining imported themes. */
  remove: (id: string) => call<Theme[]>("remove_imported_theme", { id }),
  onChanged: (handler: (themes: Theme[]) => void): Promise<UnlistenFn> =>
    listen<Theme[]>(THEMES_CHANGED_EVENT, (event) => handler(event.payload)),
};
