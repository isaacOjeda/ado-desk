// Pure theme helpers: catalog lookup, effective kind resolution and token -> CSS variable mapping.
import type { AppearanceSettings, ThemeMode } from "../lib/settings";
import builtin from "./builtin.json";
import { DEFAULT_THEME_IDS, type Theme, type ThemeColors, type ThemeKind } from "./types";

/** Built-in themes; their shape is enforced by src-tauri/tests/theme_contrast.rs. */
export const BUILTIN_THEMES: readonly Theme[] = parseThemes(builtin);

function parseThemes(raw: unknown): Theme[] {
  if (!Array.isArray(raw)) return [];
  return raw.filter(
    (t): t is Theme =>
      typeof t?.id === "string" && (t.kind === "light" || t.kind === "dark") && typeof t.colors === "object",
  );
}

/** System mode follows the OS; explicit modes win. */
export function resolveKind(mode: ThemeMode, prefersDark: boolean): ThemeKind {
  if (mode === "light" || mode === "dark") return mode;
  return prefersDark ? "dark" : "light";
}

/** Finds `id` among themes of `kind`, falling back to that kind's default, then to any theme of the kind. */
export function pickTheme(themes: readonly Theme[], kind: ThemeKind, id: string | undefined): Theme {
  const ofKind = themes.filter((t) => t.kind === kind);
  const found =
    ofKind.find((t) => t.id === id) ?? ofKind.find((t) => t.id === DEFAULT_THEME_IDS[kind]) ?? ofKind[0] ?? themes[0];
  if (!found) throw new Error("No themes available");
  return found;
}

/** The theme to paint for the given appearance settings and OS preference. */
export function resolveTheme(
  themes: readonly Theme[],
  appearance: AppearanceSettings,
  prefersDark: boolean,
): Theme {
  const kind = resolveKind(appearance.mode, prefersDark);
  return pickTheme(themes, kind, kind === "dark" ? appearance.darkTheme : appearance.lightTheme);
}

/** `contentBg` -> `--content-bg`. */
export function tokenToCssVar(token: string): string {
  return `--${token.replace(/[A-Z]/g, (c) => `-${c.toLowerCase()}`)}`;
}

/** CSS custom properties for a theme's colors, e.g. `{ "--content-bg": "#fbf9f4" }`. */
export function themeCssVariables(colors: ThemeColors): Record<string, string> {
  const vars: Record<string, string> = {};
  for (const [token, value] of Object.entries(colors)) vars[tokenToCssVar(token)] = value;
  return vars;
}

/**
 * Native vibrancy is only guaranteed on macOS. Elsewhere (Windows 10, Linux) a transparent
 * page would show the desktop through the window, so translucency stays off.
 */
export function supportsTranslucency(userAgent: string = navigator.userAgent): boolean {
  return /Macintosh|Mac OS X/i.test(userAgent);
}

/** Writes the theme onto an element (normally <html>): variables, data attributes and color-scheme. */
export function applyTheme(root: HTMLElement, theme: Theme, translucency: boolean): void {
  for (const [name, value] of Object.entries(themeCssVariables(theme.colors))) root.style.setProperty(name, value);
  root.style.colorScheme = theme.kind;
  root.dataset.theme = theme.id;
  root.dataset.themeKind = theme.kind;
  root.dataset.translucency = translucency && supportsTranslucency() ? "on" : "off";
}
