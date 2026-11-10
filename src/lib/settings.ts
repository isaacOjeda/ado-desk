// Shared settings contract between the Rust store (settings.rs) and both windows.
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { call, type Session } from "./api";

export type ThemeMode = "system" | "light" | "dark";

export interface AppearanceSettings {
  mode: ThemeMode;
  /** Theme id used when the effective mode is light. */
  lightTheme: string;
  /** Theme id used when the effective mode is dark. */
  darkTheme: string;
  /** Glass/vibrancy layers; off paints opaque surfaces. */
  translucency: boolean;
  /** How much of the desktop shows through glass layers: 0 (opaque) to 100. Default 50. */
  glassIntensity: number;
}

export interface SearchSettings {
  /** Whether the global shortcut opens the floating search bar. */
  globalShortcutEnabled: boolean;
  /** Accelerator such as `CommandOrControl+Shift+Space`; validated in Rust. */
  globalShortcut: string;
}

export interface NotificationSettings {
  /** Background checks for review requests, assignments and comments. */
  enabled: boolean;
  /** Minutes between checks, 1 to 60 (clamped in Rust). */
  intervalMinutes: number;
  reviewRequests: boolean;
  assignments: boolean;
  comments: boolean;
}

export type ViewScope = "workItems" | "pullRequests";

/** Mirrors `views::SavedView` (Rust). */
export interface SavedView {
  id: string;
  name: string;
  /** Lowercase organization name; null for the built-in starters, shown in every organization. */
  organization: string | null;
  scope: ViewScope;
  /** A stored filter; untrusted, so it goes through the filter parsers before use. */
  filter: unknown;
}

export interface Settings {
  appearance: AppearanceSettings;
  search: SearchSettings;
  notifications: NotificationSettings;
  /** Read-only here: `update_settings` keeps the stored views; use `saveView` / `deleteView`. */
  views: SavedView[];
}

/** Mirrors `views::starters` (Rust). */
export const STARTER_VIEWS: SavedView[] = [
  {
    id: "starter-review",
    name: "Waiting for my review",
    organization: null,
    scope: "pullRequests",
    filter: { project: null, status: "active", role: "reviewing" },
  },
  {
    id: "starter-closed",
    name: "My closed work items",
    organization: null,
    scope: "workItems",
    filter: { project: null, assignee: { kind: "me" }, iteration: { kind: "any" }, types: [], state: "closed" },
  },
];

export const DEFAULT_SETTINGS: Settings = {
  appearance: { mode: "system", lightTheme: "paper", darkTheme: "graphite", translucency: true, glassIntensity: 50 },
  search: { globalShortcutEnabled: true, globalShortcut: "CommandOrControl+Shift+Space" },
  notifications: { enabled: true, intervalMinutes: 5, reviewRequests: true, assignments: true, comments: true },
  views: STARTER_VIEWS,
};

/** Emitted by Rust to every window after `update_settings` persisted a change. */
export const SETTINGS_CHANGED_EVENT = "settings-changed";

export const settingsApi = {
  get: () => call<Settings>("get_settings"),
  /** Persists the full settings object and broadcasts it; resolves with the stored value. */
  update: (settings: Settings) => call<Settings>("update_settings", { settings }),
  openWindow: () => call<void>("open_settings_window"),
  /** Saves filters as a view of `organization`; resolves with the stored settings. */
  saveView: (organization: string, scope: ViewScope, name: string, filter: unknown) =>
    call<Settings>("save_view", { organization, scope, name, filter }),
  /** Removes a saved or built-in view; resolves with the stored settings. */
  deleteView: (id: string) => call<Settings>("delete_view", { id }),
  /** Why the saved global shortcut is not active (for example taken at startup), or null. */
  shortcutProblem: () => call<string | null>("global_shortcut_problem"),
  /** Registers the saved global shortcut again; resolves with the remaining problem, or null. */
  retryShortcut: () => call<string | null>("retry_global_shortcut"),
  /** Signed-in session held by Rust (no network call); null when signed out. */
  currentSession: () => call<Session | null>("current_session"),
  onChanged: (handler: (settings: Settings) => void): Promise<UnlistenFn> =>
    listen<Settings>(SETTINGS_CHANGED_EVENT, (event) => handler(event.payload)),
};
