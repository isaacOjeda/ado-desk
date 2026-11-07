# Feature: quick-search
Objective: roadmap phase 1. A Cmd+K palette in the main window that opens a work item or PR by ID and searches by title; phase 2 reuses its search for the global floating bar.
Branch: feat/quick-search · Delivery: ask-on-risk · Test runner: `cargo test` (src-tauri), `pnpm exec tsc --noEmit`, `pnpm build`.

## Specs
- S1 Request: "Me gustan todas tus sugerencias, arma un roadmap para irlas implementando una a una" + "sí, adelante". Suggestion accepted: "Buscador rápido con Cmd+K. Abrir `#1234` o `!567` directamente y buscar por título en todas las organizaciones."
- S2 (assumption) Query syntax: `#123` work item 123; `!123` pull request 123; a bare number looks up both; any other text (at least 2 characters) searches titles. IDs must be positive and at most 2147483647. Empty query: no request, the palette shows recently opened items.
- S3 (assumption) Sources: work item IDs through `_apis/wit/workitems?ids=`, PR IDs through the org-level `_apis/git/pullrequests/{id}`; work item titles through WIQL `[System.Title] CONTAINS` (value through `escape_literal`), newest change first, at most 20; PR titles have no server-side filter, so title search covers your own PRs (author or reviewer, any status, the same set the PR list fetches with role "Mine" and status "All"), filtered in Rust on every word of the query, at most 20, cached per organization for 60 s. Any PR opens by `!id`.
- S4 (assumption) Organizations: every configured organization is searched, the active one first; results show the organization and project. An organization that cannot be searched (for example an expired PAT) is skipped and named, never fails the whole search. Opening a result from another organization switches to it first, then opens the item.
- S5 (assumption) UX: Cmd/Ctrl+K opens the palette from the workspace (lists or detail). Typing searches with a 350 ms debounce; only the latest response is shown. Up/Down move, Enter opens the item in the workspace, Cmd/Ctrl+Enter opens it in Azure DevOps, Esc closes the palette (and only the palette). Recently opened items (palette or lists) are kept per device, at most 10, in localStorage, validated on read.
- S6 (assumption, security) Every request runs in Rust with the stored PATs; no new PAT scope; error messages are fixed text and never echo the query.

## Tasks
- T1 [S2, S3, S5, S6] Active organization only: Rust `search.rs` (query parser, title WIQL, PR title filter, cache, `quick_search` command), `work_items_by_ids` and `pull_request_by_id` in the client, palette UI with recents, wired into App/Workspace. Route: inline. Risk: none expected. Status: pending.
- T2 [S4] Cross-organization search and open-in-other-org handoff. Route: inline. Risk: item 2 (reads stored PATs of non-active organizations to connect). Status: pending.
- T3 [S1-S6] Independent verifier for T2 (item 2). Status: pending.

## Log
- L1 (2026-11-07) User request: "Me gustan todas tus sugerencias, arma un roadmap para irlas implementando una a una, en la 3. me gustaría si es posible tener un floating bar que aunque esté fuera de la aplicación, con un short cut, poder buscar un PR o ticket" — then "sí, adelante" (merge themes to main and start phase 1).
- L2 Evidence: REST 7.1 `GitPullRequestSearchCriteria` has no title field (Microsoft Learn); `GetPullRequestByIdAsync(int pullRequestId)` exists without project, so `_apis/git/pullrequests/{id}` works at org level. `Workspace` is keyed by organization in `App.tsx`, so opening an item of another organization needs an App-level pending open that survives the remount.
