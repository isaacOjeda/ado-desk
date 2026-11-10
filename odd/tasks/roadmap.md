# ADO Desk roadmap
Objective: turn ADO Desk from a reader into a daily driver, one feature at a time. Each phase gets its own ODD feature document (`odd/tasks/<feature>.md`) and branch when it starts.
Order is by dependency first, then value. Update the Status cell; never delete a row.

## Source
- L1 (2026-11-07) "Me gustan todas tus sugerencias, arma un roadmap para irlas implementando una a una, en la 3. me gustaría si es posible tener un floating bar que aunque esté fuera de la aplicación, con un short cut, poder buscar un PR o ticket"

## Execution order

| # | Phase | Depends on | New PAT scope | Risk items | Status |
|---|-------|-----------|---------------|------------|--------|
| 1 | Quick search (Cmd+K in the app) | — | none | — | Done (main c87b2bb, 2026-11-07) |
| 2 | Background mode + global floating search bar | 1 | none | 5 (new plugin deps) | Done (main 1f5afb0, 2026-11-07) |
| 3 | Notifications for what needs you | 2 | none | 4 (polling, async ordering) | Done (main 3e32270, 2026-11-07) |
| 4 | Saved views | — | none | 3 (persisted settings shape) | Implemented on feat/saved-views, pending merge (2026-11-10) |
| 5 | Build and policy status on PRs | — | Build (Read) | 2 (PAT scope, `Forbidden` message) | Planned |
| 6 | Write actions | 5 | Work Items + Code (Read & Write) | 1, 2 (irreversible remote changes) | Planned |

## Phases

### 1. Quick search (Cmd+K)
- Palette inside the main window: `#1234` opens a work item, `!567` opens a PR, free text searches titles.
- Searches every connected organization, active one first; results show the org and project.
- Rust: one `quick_search(query)` command. Work items by ID and by title through WIQL (`[System.Title] CONTAINS`, value through `escape_literal`).
- Known constraint (verify first): the PR search criteria of REST 7.1 have no title filter, so title search over PRs means listing PRs (active plus recent) and filtering in Rust, with a cap.
- Keyboard-first: arrows, Enter opens, Esc closes; recent items when the query is empty.
- Done when: an ID or title opens the right item from any connected org, and no request runs per keystroke without a debounce.

### 2. Background mode + global floating search bar
- Prerequisite: the app keeps running when the main window is closed. Add a tray/menu bar icon (Open, Search, Settings, Quit); closing the window hides it instead of quitting.
- `tauri-plugin-global-shortcut`: a system-wide shortcut (default `Cmd+Shift+Space`, configurable in Settings) shows a small frameless, always-on-top, translucent `search` window centered on the active screen.
- The bar reuses the phase 1 search. Enter opens the item in the main window and brings it forward, or opens it in Azure DevOps with `Cmd+Enter`. Esc or losing focus hides the bar.
- Open decision (macOS): a plain always-on-top window does not appear over full-screen apps and activates the whole app. A Spotlight-like panel needs an `NSPanel` (community crate `tauri-nspanel`). Decide at phase start: plain window first, or panel from day one.
- Settings: enable or disable the shortcut, change the key combination, and report a conflict when another app already owns it.
- Done when: from another app, the shortcut opens the bar, a search finds an item and opens it, and the bar hides without stealing focus afterwards.

### 3. Notifications for what needs you
- Rust background poller per organization (default every 5 minutes, configurable, paused when offline or on error with backoff).
- Signals: a PR where you are a reviewer with no vote yet, a work item newly assigned to you, a new comment on your PRs or work items.
- Badge counts in the sidebar and on the tray icon; native notifications through `tauri-plugin-notification`, clicking one opens the item.
- State: last-seen markers per org in the app config dir, so a restart does not replay old notifications.
- Done when: a new review request shows up within one interval, appears once, and opens the PR on click.

### 4. Saved views
- Save the current filter set of a list as a named view ("My sprint", "PRs waiting for my review"); views appear in the sidebar.
- Built on `usePersistentFilter`; stored in settings (shape change mirrored in `settings.rs` and `src/lib/settings.ts`, with loading that tolerates the old shape).
- Built-in starter views that can be removed.
- Done when: a view survives a restart, applies its filters in one click, and an old settings file still loads.

### 5. Build and policy status on PRs
- PR list: one status dot (succeeded, failed, running, none). PR detail: builds and required policies with their state, linking to the run in Azure DevOps.
- Adds the Build (Read) scope: update the `Forbidden` message in `error.rs`, the README and the login hint. Missing scope degrades to "status unavailable", never an error page.
- Done when: a PR with a failed build shows it in the list and the detail, and a PAT without Build (Read) still loads everything else.

### 6. Write actions
- Work items: add a comment, change state, change the assignee. PRs: vote (approve, approve with suggestions, wait, reject), reply to a thread, resolve a thread.
- Needs Read & Write scopes; the app must work read-only when the PAT lacks them (actions disabled with a hint).
- Every write runs in Rust, sends only validated fields, shows a confirmation for votes and state changes, and refreshes the item from the server afterwards (no optimistic writes).
- Split into two deliveries: work item actions, then PR actions.
- Done when: each action round-trips against a test project and a read-only PAT hides the actions without errors.

## Cross-cutting rules
- The webview keeps no network access; every new call lives in Rust.
- New commands are registered in `lib.rs`; errors never echo raw input or credentials.
- Each phase ends with `cargo test`, `cargo clippy`, `pnpm exec tsc --noEmit`, `pnpm build`, and a run of the real app (`pnpm tauri dev`).
