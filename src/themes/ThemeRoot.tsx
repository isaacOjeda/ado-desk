// Applies the active theme to the document and keeps it in sync with settings and the OS.
import { createContext, useContext, useEffect, useMemo, useState, type ReactNode } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { DEFAULT_SETTINGS, settingsApi, type AppearanceSettings, type Settings } from "../lib/settings";
import { themesApi } from "./api";
import { applyTheme, BUILTIN_THEMES, parseImportedThemes, resolveKind, resolveTheme } from "./tokens";
import type { Theme, ThemeKind } from "./types";

const DARK_QUERY = "(prefers-color-scheme: dark)";
/** Last applied appearance, so a relaunch paints the chosen theme before settings load. */
const CACHE_KEY = "ado-desk.appearance";
/** Last known imported themes, so a relaunch can paint an imported theme before Rust answers. */
const IMPORTED_CACHE_KEY = "ado-desk.imported-themes";

function prefersDarkNow(): boolean {
  try {
    return window.matchMedia(DARK_QUERY).matches;
  } catch {
    return true;
  }
}

function readCachedAppearance(): AppearanceSettings {
  try {
    const raw = localStorage.getItem(CACHE_KEY);
    if (raw) return { ...DEFAULT_SETTINGS.appearance, ...(JSON.parse(raw) as Partial<AppearanceSettings>) };
  } catch {
    // Storage unavailable or corrupted: defaults are fine.
  }
  return DEFAULT_SETTINGS.appearance;
}

function cacheAppearance(appearance: AppearanceSettings) {
  try {
    localStorage.setItem(CACHE_KEY, JSON.stringify(appearance));
  } catch {
    // Best effort only.
  }
}

function readCachedImported(): Theme[] {
  try {
    const raw = localStorage.getItem(IMPORTED_CACHE_KEY);
    if (raw) return parseImportedThemes(JSON.parse(raw));
  } catch {
    // Storage unavailable or corrupted: Rust's list arrives shortly.
  }
  return [];
}

function cacheImported(themes: readonly Theme[]) {
  try {
    localStorage.setItem(IMPORTED_CACHE_KEY, JSON.stringify(themes));
  } catch {
    // Best effort only.
  }
}

function catalog(imported: readonly Theme[]): readonly Theme[] {
  return imported.length ? [...BUILTIN_THEMES, ...imported] : BUILTIN_THEMES;
}

// Paint synchronously at import time (before React's first render) to avoid a flash of unstyled
// or wrong-theme content: the stylesheet has no color fallbacks of its own.
const initialAppearance = readCachedAppearance();
const initialPrefersDark = prefersDarkNow();
const initialImported = readCachedImported();
applyTheme(
  document.documentElement,
  resolveTheme(catalog(initialImported), initialAppearance, initialPrefersDark),
  initialAppearance.translucency,
  initialAppearance.glassIntensity,
);

export interface ThemeState {
  themes: readonly Theme[];
  /** Theme currently painted. */
  theme: Theme;
  /** Effective kind after resolving system mode. */
  kind: ThemeKind;
  settings: Settings;
}

const ThemeContext = createContext<ThemeState>({
  themes: catalog(initialImported),
  theme: resolveTheme(catalog(initialImported), initialAppearance, initialPrefersDark),
  kind: resolveKind(initialAppearance.mode, initialPrefersDark),
  settings: { ...DEFAULT_SETTINGS, appearance: initialAppearance },
});

/** Theme catalog and the effective theme; works without a ThemeRoot (falls back to defaults). */
export function useThemes(): ThemeState {
  return useContext(ThemeContext);
}

export function ThemeRoot({ children }: { children: ReactNode }) {
  const [settings, setSettings] = useState<Settings>(() => ({ ...DEFAULT_SETTINGS, appearance: initialAppearance }));
  const [prefersDark, setPrefersDark] = useState(initialPrefersDark);
  const [imported, setImported] = useState<readonly Theme[]>(initialImported);

  useEffect(() => {
    let active = true;
    const accept = (themes: unknown) => {
      if (!active) return;
      const parsed = parseImportedThemes(themes);
      setImported(parsed);
      cacheImported(parsed);
    };
    themesApi
      .list()
      .then(accept)
      .catch(() => {});

    let unlisten: (() => void) | undefined;
    try {
      themesApi
        .onChanged(accept)
        .then((stop) => (active ? (unlisten = stop) : stop()))
        .catch(() => {});
    } catch {
      // Not running inside Tauri: built-in themes only.
    }
    return () => {
      active = false;
      unlisten?.();
    };
  }, []);

  useEffect(() => {
    let active = true;
    settingsApi
      .get()
      .then((loaded) => active && setSettings(loaded))
      .catch(() => active && setSettings(DEFAULT_SETTINGS));

    let unlisten: (() => void) | undefined;
    try {
      settingsApi
        .onChanged((next) => active && setSettings(next))
        .then((stop) => (active ? (unlisten = stop) : stop()))
        .catch(() => {});
    } catch {
      // Not running inside Tauri: no live updates.
    }
    return () => {
      active = false;
      unlisten?.();
    };
  }, []);

  useEffect(() => {
    let media: MediaQueryList;
    try {
      media = window.matchMedia(DARK_QUERY);
    } catch {
      return;
    }
    const onChange = (event: MediaQueryListEvent) => setPrefersDark(event.matches);
    setPrefersDark(media.matches);
    media.addEventListener("change", onChange);
    return () => media.removeEventListener("change", onChange);
  }, []);

  const { appearance } = settings;
  const kind = resolveKind(appearance.mode, prefersDark);
  const themes = useMemo(() => catalog(imported), [imported]);
  const theme = useMemo(() => resolveTheme(themes, appearance, prefersDark), [themes, appearance, prefersDark]);

  useEffect(() => {
    applyTheme(document.documentElement, theme, appearance.translucency, appearance.glassIntensity);
    cacheAppearance(appearance);
  }, [theme, appearance]);

  useEffect(() => {
    // Native appearance (vibrancy material, title bar). System mode passes null so the webview keeps
    // following the OS and `prefers-color-scheme` stays live.
    try {
      void getCurrentWindow()
        .setTheme(appearance.mode === "system" ? null : appearance.mode)
        .catch(() => {});
    } catch {
      // Outside Tauri.
    }
  }, [appearance.mode]);

  const value = useMemo<ThemeState>(
    () => ({ themes, theme, kind, settings }),
    [themes, theme, kind, settings],
  );

  return <ThemeContext.Provider value={value}>{children}</ThemeContext.Provider>;
}
