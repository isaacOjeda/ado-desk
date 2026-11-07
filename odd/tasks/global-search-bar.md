# Feature: global-search-bar
Objective: roadmap phase 2. Keep ADO Desk running in the background and open a floating search bar from anywhere with a global shortcut, reusing the phase 1 quick search.
Branch: feat/global-search-bar · Delivery: ask-on-risk · Test runner: `cargo test` (src-tauri), `pnpm exec tsc --noEmit`, `pnpm build`.

## Specs
- S1 Request: "en la 3. me gustaría si es posible tener un floating bar que aunque esté fuera de la aplicación, con un short cut, poder buscar un PR o ticket". Window type chosen by the user: "let go for normal windows" (no NSPanel; the bar does not appear over full-screen apps).
- S2 (assumption) Background mode: closing the main window hides it and the app keeps running. A menu bar (tray) icon offers Open ADO Desk, Search…, Settings…, Quit. Clicking the Dock icon shows the main window again. Quit and Cmd+Q exit.
- S3 (assumption) Floating bar: the global shortcut (default `CommandOrControl+Shift+Space`) shows a frameless, always-on-top, translucent `search` window, horizontally centered in the upper part of the screen under the cursor; pressing it again while the bar is focused hides it. Same search as the palette (syntax, every organization, recents). Enter hides the bar, shows the main window and opens the item (switching organization when needed); Cmd/Ctrl+Enter opens it in Azure DevOps; Esc or losing focus hides the bar.
- S4 (assumption) Settings: Settings shows the global search shortcut with an on/off switch and a recorder for a new combination (at least one modifier plus a key). A combination that cannot be registered (invalid, or taken by another app) is reported and the previous one stays active. Stored in `settings.json`; files without the field load with the default.
- S5 (assumption) Each organization's quick search has a 10 s deadline; a slower organization is reported as unavailable instead of holding back the others (follow-up from the phase 1 review).
- S6 (assumption, security) The bar uses the same CSP and capability as the other windows; the item it sends to the main window is validated again before it opens; no new PAT scope; the shortcut string is validated in Rust before registration and never echoed in errors.

## Tasks
- T1 [S5] Per-organization search deadline. Route: inline. Risk: item 5 (declares `tokio` with `time`, already in the dependency tree; Cargo.lock +1 line). Status: done, see L3.
- T2 [S2] Background mode: tray icon and menu, close hides, Dock reopen. Route: inline. Risk: item 5 (Tauri `tray-icon` feature). Status: pending.
- T3 [S3, S6] Global shortcut + `search` window + open in main window. Route: inline. Risk: items 2 (new window in the capability) and 5 (`tauri-plugin-global-shortcut`). Status: pending.
- T4 [S4] Shortcut settings (Rust validation and registration, settings shape, recorder UI). Route: inline. Risk: none expected (optional new settings field). Status: pending.
- T5 [S1-S6] Independent verifier (items 2, 5). Status: pending.

## Log
- L1 (2026-11-07) User request: "en la 3. me gustaría si es posible tener un floating bar que aunque esté fuera de la aplicación, con un short cut, poder buscar un PR o ticket"; phase 1 merged ("yes"); window type: "let go for normal windows".
- L2 Evidence: Tauri 2.12.5, tokio 1.54.1 already in Cargo.lock; capability `default` lists windows `main`, `settings`; opener only allows `https://*`; Settings window is created from its `tauri.conf.json` entry (`create: false`) by `open_settings_window`.
- L3 T1: RED 1/1 (`with_deadline` stub) then GREEN; cargo test 158 + 5 ok, clippy clean. Each organization in `quick_search` runs under `ORGANIZATION_DEADLINE` (10 s); past it the organization lands in `unavailable` (Network error, fixed text) and the others answer.
