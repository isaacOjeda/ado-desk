# Feature: multi-org-filters
Objective: several Azure DevOps organizations (one PAT each, one active at a time) and advanced filters for work items and pull requests.
Branch: feat/multi-org-filters (stacked on feat/appearance-settings) · Delivery: ask-on-risk · Test runner: `cargo test` (src-tauri), `pnpm exec tsc --noEmit`, `pnpm build`.

## Specs
- S1 Multiple organizations, each with its PAT: "Me gustaría que fuera multi organización, poder configurar más orgs con su pat"
- S2 One active organization at a time with a switcher (user decision, verbatim answer): "Cambiar entre orgs (Recommended)" — "Una organización activa a la vez, con un selector en la barra lateral (como los workspaces de Slack). Los filtros de proyecto, sprint, asignado, etc. se aplican a la org activa."
- S3 Work item filters: "agregar filtros avanzados, filtrar por proyecto, asignado, sprint, tipo de ítem"
- S4 Pull request filters: "en PRs algo similar, por proyecto, ver PRs cerrados o abandonados"
- S5 (derived constraints) The organization already signed in keeps working after the upgrade (stored credential migrated, never lost); every PAT lives only in the OS keychain and never reaches the webview; adding an org validates its PAT before storing it; removing an org deletes its PAT.
- S6 (assumptions) Default filters reproduce today's lists (open work items assigned to me; my active PRs). Assignee options: Me, Anyone, Unassigned, or a specific person. Sprint options: Any, Current (of the project's default team), or a specific iteration. Filters are remembered per organization on this device.

## Tasks
- T1 [S1,S2,S5] Multi-organization: credential store with migration, per-org clients, add/switch/remove commands, sidebar org switcher, add-org flow, Settings Profile list. Route: delegated writer (parallel, worktree).
- T2 [S3,S4,S6] Filters: typed filter queries (WIQL built safely in Rust, PR search criteria by status/project), project/iteration/type lookups, filter bar UI for both lists. Route: delegated writer (parallel, worktree).
- T3 Seam merge, checks, visual check, verifier (Risk items 1 and 2 for T1).

## Log
- L1 (2026-11-06) User request: "Me gustaría que fuera multi organización, poder configurar más orgs con su pat // También me gustaría agregar filtros avanzados, filtrar por proyecto, asignado, sprint, tipo de ítem, y en PRs algo similar, por proyecto, ver PRs cerrados o abandonados"
- L2 Question asked (view model for several orgs); answer: "Cambiar entre orgs (Recommended)".
- L3 Route evidence: T1/T2 independent behaviors, mostly disjoint surfaces (shared only lib.rs handler list, resolved at merge); parallel worktrees; parent context past the long-session backstop.
