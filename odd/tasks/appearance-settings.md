# Feature: appearance-settings
Objective: reading-focused light/dark themes, macOS glass aesthetic, and a Settings window (Profile, Appearance).
Branch: feat/appearance-settings (stacked on feat/detail-views) · Delivery: ask-on-risk · Test runner: `cargo test` (src-tauri), `pnpm exec tsc --noEmit`, `pnpm build`.

## Specs
- S1 Reading focus: "me gustaría enfocarme en la lectura"
- S2 Light mode in addition to dark: "me gusta el modo dark pero también quiero modo light"
- S3 Reading-oriented palettes (WCAG AA or better for body text, low glare): "me gustaría usar un esquema de colores pensado para leer"
- S4 Glass aesthetic: "una estética tipo glass de macOS (glass, acrilico, etc)"
- S5 Settings window with sections: "quisiera una ventana de settings, donde tengamos varias secciones"
- S6 Profile section: "que salga nuestro perfil (quienes somos y la organización que tenemos configurada)"
- S7 Appearance section: "una de apariencia, donde saldrá el color default que elijamos y aparte poder cambiar temas"
- S8 Out of scope now, design for later: "a futuro quisiera poder importaer temas de Open VSX". Assumption: themes are data (JSON tokens) whose keys map from VS Code color theme `colors`.
- S9 (assumptions) Settings opens as a separate native window from a sidebar gear button and Cmd/Ctrl+,; mode System/Light/Dark with a light and a dark theme choice; translucency can be turned off; settings persist in the app config dir and sync live across windows.

## Contract (shared by T2/T3)
- `src/lib/settings.ts`: `Settings`, `DEFAULT_SETTINGS`, `settingsApi` (commands `get_settings`, `update_settings`, `open_settings_window`, `current_session`; event `settings-changed`).
- `src/themes/ThemeRoot.tsx` (owned by T3) wraps both windows; `src/features/settings/AppearanceSection.tsx` (owned by T3) is rendered by the Settings window (owned by T2).
- Default themes: light `paper`, dark `graphite`.

## Tasks
- T1 [S5-S9] Contract files and stubs. Route: inline. Status: done, commit fccf7fa.
- T2 [S4,S5,S6,S9] Rust settings store + events, settings window, glass window config, Profile section, gear button/shortcut. Route: delegated writer (parallel, worktree). Status: done, commit 6a445e0 (cargo test 61 ok, clippy clean, tauri build ok).
- T3 [S1,S2,S3,S4,S7,S8] Theme data + contrast test, ThemeRoot, token-based CSS across the app with glass layers, Appearance section. Route: delegated writer (parallel, worktree). Status: done, commit 15be3af (contrast test 5 ok; ratios text ≥12.19:1, muted ≥6.11:1).
- T4 Seam check, visual check, verifier (items 2, 3). Status: done.

## Log
- L1 (2026-11-06) User request: "todo va genial, me gustaría enfocarme en la lectura, me gusta el modo dark pero también quiero modo light, y me gustaría usar un esquema de colores pensado para leer // esto me lleva a también tener una estética tipo glass de macOS (glass, acrilico, etc) y quisiera una ventana de settings, donde tengamos varias secciones, por ahora, que salga nuestro perfil (quienes somos y la organización que tenemos configurada) y una de apariencia, donde saldrá el color default que elijamos y aparte poder cambiar temas, por ahora con eso (a futuro quisiera poder importaer temas de Open VSX)"
- L2 Route evidence: T2/T3 independent behaviors, disjoint surfaces, each several files; parallel worktrees (same Rust crate / shared CSS). Parent context past the long-session backstop.
- L3 Seam check on merged branch: cargo test 61+5 ok, clippy clean, tsc ok, pnpm build ok, tauri debug build ok. Browser preview (mocked IPC) in light/dark: list, work item detail, Settings Profile and Appearance render as specified. Parent fixes: translucency only on macOS (Windows 10/Linux would show the desktop through a transparent page); detail properties panel stacks under the reading column below 1080px.
- L4 Independent verifier (items 2 and 3): no blockers; S1-S9 PASS (S4/S6 visuals and email source unverifiable offline); 33/33 CSS variables defined; 0 hardcoded colors outside builtin.json. Applied advisories 1 (theme ids limited to [a-z0-9-], max 64, RED observed first) and 2 (Settings window glass gated by data-translucency). Accepted: midnight faint/accent-label contrast kept as today's palette; App Store limitation of macOSPrivateApi. cargo test 63+5 ok, clippy clean, build ok. RDD assess: medium, under_budget.
