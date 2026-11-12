# Feature: pr-actions
Objective: pull request write actions (vote, comment on selected diff lines) and a Markdown preview for `.md` files in the PR files view. Roadmap phase 6, PR delivery.
Branch: feat/improvements · Delivery: ask-on-risk · Test runner: `cargo test` (src-tauri), `pnpm exec tsc --noEmit`, `pnpm build`.

## Specs
- S1 Comment on selected diff lines: "quiero poder comentar en PRs en lineas que seleccione". Selecting one line or a range of lines in a file diff opens a comment box; saving creates a new thread anchored to that file and line range.
- S2 Markdown preview: "si los archivos son .md quiero poder ver un preview". A `.md` file in the PR files view can switch between its diff and a rendered preview of the changed version (the original version when the file was deleted).
- S3 Votes: "no sé si se pueden emitir votos o no, pero también quisiera eso en los PRs". The signed-in user can approve, approve with suggestions, wait for author, reject, or reset their vote, with a confirmation (roadmap phase 6).
- S4 (derived, mirrors work-item-actions S2-S5) Writes run in Rust through `AzdoClient::write`; failures never sign out and never echo server text or the values sent; comments are sent as typed (Markdown, like the web), trimmed, 1-4000 characters; the PR reloads from the server after every write (no optimistic writes); a token without Code (Read & Write) keeps the app working read-only, with a hint instead of errors.
- S5 (derived, pr-checks S1) New optional scope Code (Read & Write): update the `Forbidden` message in `error.rs`, the README, the login hint and CLAUDE.md.
- S6 (derived, detail-views S9) The preview Markdown is sanitized in Rust; the PAT never reaches the webview. (Assumption) Repository-relative images in a preview are not loaded.
- S7 Amends detail-views S8 ("Read-only") for pull requests.

## Tasks
- T1 [S2, S6] `.md` preview: Rust command returning sanitized HTML for one side of a changed file, Diff/Preview toggle. Route: inline. Status: done (RED 3/3 then GREEN; cargo test 252 + 5 ok, clippy clean, tsc + build ok; not exercised in the running app).
- T2 [S3, S4, S5, S7] Votes: Code write-access probe, `vote_pull_request`, vote control in Overview. Route: inline. Status: done (no probe: Azure DevOps has no validate-only PR write, so a refused vote switches that detail to read-only; RED 4/4 then GREEN; cargo test 256 + 5 ok, clippy clean, tsc + build ok; not exercised against Azure DevOps).
- T3 [S1, S4] Line comments: selection in the diff, `add_pull_request_line_comment`, comment box, reload threads. Route: inline. Status: done (RED 3/3 then GREEN; cargo test 259 + 5 ok, clippy clean, tsc + build ok; not exercised against Azure DevOps).

## Log
- L1 (2026-11-11) User request: "crea una rama y pushea estos cambios pero quiero más cambios así que el nombre que sea features generico / quiero poder comentar en PRs en lineas que seleccione / si los archivos son .md quiero poder ver un preview / no sé si se pueden emitir votos o no, pero también quisiera eso en los PRs"
- L2 Branch feat/improvements created from main a15715b; Inbox filters committed as 8d313f3 and pushed.
- L3 Route evidence: units share the PR detail files (Files.tsx, Diff.tsx, pull_requests.rs, client.rs), so no parallel writers; inline, one commit per task.
