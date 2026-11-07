// Theme data model. Built-in themes live in `builtin.json`; every key in `ThemeColors`
// becomes a CSS custom property on <html> (camelCase -> --kebab-case, see `tokens.ts`).
//
// Open VSX / VS Code import: a VS Code color theme is a JSON file with a `type` and a
// `colors` map. The `vscode` column below names the `colors` entries the importer
// (src-tauri/src/themes/vscode.rs) reads for each key, first match wins; keys marked
// "derived" have no direct equivalent and are computed (mixing or alpha).
//
// Colors are hex strings: `#rrggbb` for opaque tokens (required for every token the
// contrast test checks) and `#rrggbbaa` for translucent layers, shadows and diff tints.

export type ThemeKind = "light" | "dark";

export interface ThemeColors {
  /** Window background and opaque fallback for chrome. vscode: `editorGroupHeader.tabsBackground`, `titleBar.activeBackground`, `editor.background`. */
  bg: string;
  /** Sidebar, opaque. vscode: `sideBar.background`, `activityBar.background`. */
  sidebar: string;
  /** Reading surface: lists, descriptions, comments, diffs. vscode: `editor.background`. */
  contentBg: string;
  /** Raised controls and cards (inputs, segmented controls, PR threads). vscode: `input.background`, `editorWidget.background`. */
  surface: string;
  /** Hover / selected rows. vscode: `list.hoverBackground`, `list.inactiveSelectionBackground`. */
  surfaceHover: string;
  /** Hairlines. vscode: `panel.border`, `editorGroup.border`, `sideBar.border`. */
  border: string;
  /** Emphasized borders. vscode: `input.border`, `contrastBorder`, else derived from `border`. */
  borderStrong: string;
  /** Body text. vscode: `editor.foreground`, `foreground`. */
  text: string;
  /** Secondary text. vscode: `descriptionForeground`. */
  textMuted: string;
  /** Tertiary text (ids, timestamps, labels). vscode: `editorLineNumber.foreground`, `disabledForeground`. */
  textFaint: string;
  /** Links, focus rings, primary buttons. vscode: `textLink.foreground`, `focusBorder`, `button.background`. */
  accent: string;
  /** Accent hover / link color. vscode: `textLink.activeForeground`, `button.hoverBackground`. */
  accentStrong: string;
  /** Text drawn on top of `accent` (primary buttons, avatars). vscode: `button.foreground`. */
  accentContrast: string;
  /** Translucent sidebar layer over the native vibrancy. Derived: `sidebar` with ~70% alpha. */
  glassSidebar: string;
  /** Translucent toolbars and detail headers. Derived: `bg` with ~75% alpha. */
  glassChrome: string;
  /** Drop shadow color for floating cards. vscode: `widget.shadow`. */
  shadow: string;
  /** Brand gradient start (logo mark, avatars). Derived: `accent`. */
  brandStart: string;
  /** Brand gradient end. Derived: `terminal.ansiMagenta`. */
  brandEnd: string;
  /** Added lines in diffs. vscode: `diffEditor.insertedLineBackground`, `diffEditor.insertedTextBackground`. */
  diffInserted: string;
  /** Removed lines in diffs. vscode: `diffEditor.removedLineBackground`, `diffEditor.removedTextBackground`. */
  diffRemoved: string;
  /** Neutral status. vscode: `descriptionForeground`. */
  toneNeutral: string;
  /** vscode: `terminal.ansiBlue`. */
  toneBlue: string;
  /** vscode: `terminal.ansiGreen`, `gitDecoration.addedResourceForeground`. */
  toneGreen: string;
  /** vscode: `terminal.ansiYellow`, `editorWarning.foreground`. */
  toneAmber: string;
  /** vscode: `terminal.ansiRed`, `errorForeground`. */
  toneRed: string;
  /** vscode: `terminal.ansiMagenta`. */
  toneViolet: string;
  /** vscode: `gitDecoration.modifiedResourceForeground`, `terminal.ansiBrightYellow`. */
  toneOrange: string;
}

export type ThemeToken = keyof ThemeColors;

/** The Open VSX extension an imported theme came from. */
export interface ThemeSource {
  namespace: string;
  name: string;
  displayName: string;
  version: string;
}

export interface Theme {
  id: string;
  name: string;
  /** vscode: `uiTheme` / `type` ("light" | "dark"; high-contrast variants map to their base). */
  kind: ThemeKind;
  colors: ThemeColors;
  /** Only on imported themes (ids start with `ovsx-`). */
  source?: ThemeSource;
}

/** Defaults per kind; mirrors `DEFAULT_SETTINGS` in src/lib/settings.ts. */
export const DEFAULT_THEME_IDS: Record<ThemeKind, string> = { light: "paper", dark: "graphite" };
