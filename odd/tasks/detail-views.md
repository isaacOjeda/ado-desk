# Feature: detail-views
Objective: read-only in-app detail views for work items and pull requests, similar to the Azure DevOps web.
Branch: feat/detail-views · Delivery: ask-on-risk · Test runner: `cargo test` (src-tauri), `pnpm exec tsc --noEmit`, `pnpm build`.

## Specs
- S1 Work item detail opens inside the app: "podemos que podamos ver los tickets?"
- S2 Work item comments: "y leer comentarios?"
- S3 Work item details (fields, description, repro steps, acceptance criteria, related links): "y los detalles de los tickets?"
- S4 Pull request detail opens inside the app (description, reviewers and votes, status, branches): "igual los PRs? ver el PR"
- S5 Pull request comments (discussion threads, including file-anchored threads): "sus comentarios"
- S6 Pull request code changes (changed files list and per-file diff): "y código?"
- S7 Layout similar to the Azure DevOps web: "así como se ve similar en la web?"
- S8 (assumption) Read-only: no posting, voting or editing. The user only asked to see and read.
- S9 (derived constraint) Rich text from Azure DevOps (HTML and Markdown) is sanitized in Rust before reaching the webview; authenticated images load through a Rust proxy restricted to the signed-in organization on dev.azure.com; the PAT never reaches the webview.

## Tasks
- T1 [S7,S9] Foundation: rich-text sanitization, image proxy protocol, client helpers, detail navigation, shared comment UI, command stubs. Route: inline (shared files). Status: done (cargo test 17 ok, tsc ok, pnpm build ok).
- T2 [S1,S2,S3,S7] Work item detail (Rust module + UI). Route: delegated writer (parallel with T3, disjoint surfaces). Status: done, commit 0f9d7d0 (cargo test 24 ok, clippy clean, tsc ok, build ok in worktree).
- T3 [S4,S5,S6,S7] Pull request detail with threads and diff (Rust module + UI). Route: delegated writer (parallel with T2). Status: done, commit 5d7c3cd (cargo test 42 ok in worktree; merged branch 49 ok, clippy clean, tsc ok, tauri build ok).
- T4 [S1-S9] Seam check, independent security verification (Risk item 2), commits. Route: inline + verifier. Status: done (verifier: no blockers; AD1-AD3 applied).

## Log
- L1 (2026-11-06) User request: "genial, ya entré y funciona, podemos que podamos ver los tickets? y leer comentarios? y los detalles de los tickets? igual los PRs? ver el PR, sus comentarios y código? así como se ve similar en la web?"
- L2 Baseline commit ea88f76 on main; branch feat/detail-views. RDD on (global).
- L3 Route evidence: T2/T3 are independent behaviors with disjoint edit surfaces, each several files and well over 100 lines; launched together as parallel writers. Parent owns shared files (lib.rs, client.rs, commands.rs, Workspace.tsx, styles.css, ui.tsx, tauri.conf.json).
- L4 T1 committed 1a1fb5f (RDD assess: medium, under_budget → pending in slice). T2/T3 launched in isolated git worktrees because both compile the same Rust crate; each commits on its worktree branch, parent merges.
- L5 Seam check on merged branch: cargo test 49 passed, clippy 0 warnings, tsc ok, pnpm build ok, tauri debug build ok. Browser preview with mocked IPC: work item detail, PR overview and PR files/diff render as specified; fixed a shared layout bug (sidebar footer pushed off-screen by long detail views). Independent security verifier (Risk item 2) running.
- L6 Independent verifier: no blockers; S1-S6, S8, S9 PASS on mapping (live API unverifiable offline), S7 verified visually with mocked IPC. Applied advisories AD1 (proxy scheme no longer allowed in sanitizer; whitespace-obfuscated proxy URLs dropped), AD2 (attachment paths limited to the two real shapes, encoded separators rejected), AD3 (proxy serves images only, infers type from file name for octet-stream, CSP sandbox, 20 MB cap). RED observed for AD1/AD2 tests before the fix. AD4 (external images, http links) kept as an accepted tradeoff. cargo test 53 ok, clippy clean.
