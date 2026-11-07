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

export interface Settings {
  appearance: AppearanceSettings;
  search: SearchSettings;
}

export const DEFAULT_SETTINGS: Settings = {
  appearance: { mode: "system", lightTheme: "paper", darkTheme: "graphite", translucency: true, glassIntensity: 50 },
  search: { globalShortcutEnabled: true, globalShortcut: "CommandOrControl+Shift+Space" },
};

/** Emitted by Rust to every window after `update_settings` persisted a change. */
export const SETTINGS_CHANGED_EVENT = "settings-changed";

export const settingsApi = {
  get: () => call<Settings>("get_settings"),
  /** Persists the full settings object and broadcasts it; resolves with the stored value. */
  update: (settings: Settings) => call<Settings>("update_settings", { settings }),
  openWindow: () => call<void>("open_settings_window"),
  /** Why the saved global shortcut is not active (for example taken at startup), or null. */
  shortcutProblem: () => call<string | null>("global_shortcut_problem"),
  /** Signed-in session held by Rust (no network call); null when signed out. */
  currentSession: () => call<Session | null>("current_session"),
  onChanged: (handler: (settings: Settings) => void): Promise<UnlistenFn> =>
    listen<Settings>(SETTINGS_CHANGED_EVENT, (event) => handler(event.payload)),
};
