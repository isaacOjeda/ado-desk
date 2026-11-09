# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

ADO Desk is a Tauri 2 desktop app (Rust backend + React 19/TypeScript frontend, Vite, pnpm) that shows Azure Boards work items and Azure Repos pull requests across one or more Azure DevOps organizations.

## Commands

```bash
pnpm install
pnpm tauri dev                 # run the app (first build compiles Rust, takes minutes)
pnpm build                     # tsc + vite build of the frontend
pnpm exec tsc --noEmit         # frontend type check

cd src-tauri
cargo test                     # all Rust unit tests + tests/theme_contrast.rs
cargo test <name_substring>    # single test or module, e.g. `cargo test queries::`
cargo test --test theme_contrast
cargo clippy
```

There is no frontend test runner; frontend checks are `tsc --noEmit` and `pnpm build`. Rust logic is kept in pure functions so it can be unit tested (`#[cfg(test)]` modules inside each file).

## Architecture

**Security boundary: the webview has no network access.** The CSP (`src-tauri/tauri.conf.json`) only allows IPC and the `adoimg` scheme. Every Azure DevOps call happens in Rust; PATs enter via `add_organization`/`login` and never travel back to the frontend. Error messages (`src-tauri/src/error.rs`) must never contain credentials or echo raw user input.

**Rust side (`src-tauri/src/`)**
- `lib.rs` — builder: `AppState` (connected organizations + keychain store), `SettingsState`, the `adoimg` URI scheme, and the `generate_handler!` list. New commands must be registered here.
- `commands.rs` — session/organization lifecycle, settings, settings window. `current_client` resolves the active organization's `AzdoClient`; other modules (`queries.rs`, `work_items.rs`, `pull_requests.rs`) use it.
- `azdo/client.rs` — thin HTTP adapter over REST API 7.1; `azdo/model.rs` — domain types and pure payload mapping.
- `credentials.rs` — `OrganizationStore` over a `SecretStore` trait (keyring in prod, `MemoryStore` in tests). Keychain layout: `org:{name}` holds one PAT, `organizations` holds a non-secret index `{ active, organizations }` with lowercase names; the legacy `azure-devops` entry is migrated on first read.
- `queries.rs` — filtered lists and filter lookups. WIQL / PR criteria are built by pure functions; every user value goes through `escape_literal` before reaching a WIQL string.
- `rich_text.rs` — all HTML/Markdown sanitization (ammonia, pulldown-cmark) happens in Rust. Attachment image `src` is rewritten to `adoimg://`, served by `image_proxy.rs`, which only fetches attachment paths of the active organization (one org's PAT is never sent to another).
- `themes/` — Open VSX theme import, the only network call outside Azure DevOps: own HTTP client (HTTPS, open-vsx.org links only, size caps), in-memory vsix reader (JSONC, `include`), VS Code `colors` -> `ThemeColors` mapping (`vscode.rs`, mirrors the `vscode` column in `src/themes/types.ts`), and `imported-themes.json` store broadcast via `themes-changed`. Every imported color is strict `#rrggbb(aa)`.
- `settings.rs` — JSON in the app config dir; shape mirrors `src/lib/settings.ts`. Loading never fails (defaults on missing/corrupt), saving is atomic (temp + rename).

**Frontend (`src/`)**
- `lib/api.ts` — typed `invoke` wrappers; Rust `AppError` arrives as `{ kind, message }` and is rethrown as `ApiError`. Feature folders may have their own `api.ts`.
- `lib/useResource.ts` — data-loading hook; it refetches only when the `fetcher` identity changes, so fetchers must be memoized with the right deps.
- `main.tsx` — one bundle, two windows: `index.html#settings` renders `SettingsApp`, otherwise `App`. Both are wrapped in `ThemeRoot`.
- `features/*` — screen-level folders (auth, workspace, workitem, pullrequest, filters, organizations, settings), each with its own CSS.
- Cross-window sync uses Tauri events (`session-changed`, `settings-changed`); `app.emit` reaches every window including the sender. In-window signals that cannot go through props use small listener modules (e.g. `features/organizations/events.ts`).
- Themes: `src/themes/builtin.json` + `types.ts` (`ThemeColors`). Adding/renaming a token requires updating the `TOKENS` list in `src-tauri/tests/theme_contrast.rs`, which also enforces WCAG contrast for every built-in theme.

The README's architecture section predates multi-organization support; trust the code.

## Workflow notes

- Feature specs and task logs live in `odd/tasks/<feature>.md` (Specs `S#`, Tasks, Log). Read the relevant one before changing a feature it covers.
- Required PAT scopes: Work Items (Read), Code (Read), Project and Team (Read). Adding a call that needs another scope also means updating the `Forbidden` message in `error.rs` and the README.
