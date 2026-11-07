# Feature: openvsx-theme-import
Objective: search Open VSX from Settings > Appearance, import VS Code color themes, and use them like the built-in themes.
Branch: feat/openvsx-theme-import · Delivery: ask-on-risk · Test runner: `cargo test` (src-tauri), `pnpm exec tsc --noEmit`, `pnpm build`.

## Specs
- S1 Importer: "tambien quisiera agergar un importador de colores de Open VSX"
- S2 Discovery: user chose "Buscar en la app (Recommended)": a search box in Settings > Appearance queries Open VSX (category Themes), lists results, and imports one with a click.
- S3 (assumption) An import brings every color theme in the extension (`contributes.themes`). Imported themes appear in the existing Light/Dark pickers by kind (`vs`/`hc-light` -> light, `vs-dark`/`hc-black` -> dark; the theme JSON `type` wins when present) and can be removed. Removing the selected theme falls back to the default of its kind (`pickTheme`).
- S4 (assumption, security) The webview keeps no network access: search and download run in Rust with a client separate from `AzdoClient`, only against `https://open-vsx.org`, with a timeout and caps on download and decompressed sizes. The archive is read in memory by entry name, never extracted to disk. Every color is normalized to strict `#rrggbb`/`#rrggbbaa` before it reaches the webview; names are length-bounded; errors never echo raw input.
- S5 (assumption) Mapping follows the `vscode` column in `src/themes/types.ts`, first match wins; JSONC (comments, trailing commas), short hex (`#fff`, `#0000`) and one-level-deep `include` inside the archive are supported. Opaque tokens are flattened over the theme background; tokens with no source are derived or taken from the built-in default theme of the same kind, so every imported theme defines every token.
- S6 (assumption) Imported themes persist in `imported-themes.json` in the app config dir (atomic write) and sync live to every window through a `themes-changed` event. Ids are `ovsx-` slugs (`[a-z0-9-]`, max 64), never colliding with built-in ids. README documents the feature and that it is the first network call outside Azure DevOps.

## Tasks
- T0 [glass] Whole-window glass (separate small request). Route: inline. Status: done, commit 72524d6.
- T1 [S3-S6] Rust `themes` module: JSONC reader, vsix reader, VS Code mapping, store, Open VSX client, commands + event, error variants, unit tests. Route: inline (frontend depends on its contract, so units are not independent). Risk: items 2 and 5.
- T2 [S2,S3,S6] Frontend: themes API, ThemeRoot merges imported themes and listens to `themes-changed`, import panel with search in Appearance, remove action, README. Route: inline.
- T3 [S1-S6] Independent verifier (items 2, 5), seam check.

## Log
- L1 (2026-11-06) User request: "quisiera que en la apariencia glass, se aplicara a toda la ventana, no solo a los paneles // tambien quisiera agergar un importador de colores de Open VSX"
- L2 (2026-11-06) User chose "Buscar en la app (Recommended)" for discovery.
- L3 API evidence (live): `GET https://open-vsx.org/api/-/search?query=..&category=Themes&size=N` returns `extensions[]` with `namespace`, `name`, `displayName`, `description`, `version`, `files.download`; `GET /api/{namespace}/{name}` returns the same `files.download`. Download URLs live on `https://open-vsx.org/api/...`. vsix = zip with `extension/package.json` -> `contributes.themes[] {label, uiTheme, path}`. Probe of 8 popular themes: Nord and Tokyo Night are JSONC; GitHub and Dracula have no `type`; One Dark Pro/Gruvbox use short hex; GitHub themes contain non-string values in `colors`; no `include` seen; largest vsix 972KB, theme files ~75KB.
- L4 Glass (T0): `.content` transparent; `.panel`, `.detail-body`, `.settings-content` use `--glass-content` = contentBg with alpha 0.85 at intensity 50, 0.70 at 100. tsc + build ok. Native vibrancy not visually verified (browser preview cannot render it).
