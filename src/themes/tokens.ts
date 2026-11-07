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

/**
 * Opacity of a glass layer for a 0-100 intensity. At 50 it reproduces the original fixed
 * values (sidebar 0.70, chrome 0.76); at 100 layers keep a legibility floor (0.40 / 0.52).
 */
export function glassAlpha(layer: "sidebar" | "chrome", intensity: number): number {
  const amount = Math.min(100, Math.max(0, Number.isFinite(intensity) ? intensity : 50)) / 100;
  const maxTransparency = layer === "sidebar" ? 0.6 : 0.48;
  return Math.round((1 - amount * maxTransparency) * 1000) / 1000;
}

/** Replaces the alpha of a `#rrggbb` / `#rrggbbaa` color, returning `rgb(r g b / a)`. */
export function withAlpha(hex: string, alpha: number): string {
  const match = /^#([0-9a-f]{6})([0-9a-f]{2})?$/i.exec(hex.trim());
  if (!match) return hex;
  const n = parseInt(match[1]!, 16);
  return `rgb(${(n >> 16) & 255} ${(n >> 8) & 255} ${n & 255} / ${alpha})`;
}

/** Writes the theme onto an element (normally <html>): variables, data attributes and color-scheme. */
export function applyTheme(root: HTMLElement, theme: Theme, translucency: boolean, glassIntensity = 50): void {
  for (const [name, value] of Object.entries(themeCssVariables(theme.colors))) root.style.setProperty(name, value);
  root.style.setProperty("--glass-sidebar", withAlpha(theme.colors.glassSidebar, glassAlpha("sidebar", glassIntensity)));
  root.style.setProperty("--glass-chrome", withAlpha(theme.colors.glassChrome, glassAlpha("chrome", glassIntensity)));
  root.style.colorScheme = theme.kind;
  root.dataset.theme = theme.id;
  root.dataset.themeKind = theme.kind;
  root.dataset.translucency = translucency && supportsTranslucency() ? "on" : "off";
}
