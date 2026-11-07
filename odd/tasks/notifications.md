# Feature: notifications
Objective: roadmap phase 3. While ADO Desk runs (also in the background), check every organization on an interval and tell the user what needs them: native notifications, an Inbox view and unread badges.
Branch: feat/notifications · Delivery: ask-on-risk · Test runner: `cargo test` (src-tauri), `pnpm exec tsc --noEmit`, `pnpm build`.

## Specs
- S1 Request: proposal accepted with "that works": "Notify on new review requests on PRs, work items newly assigned to you, and new comments on your PRs or work items. Check every 5 minutes, adjustable in Settings." Before that, the phase 2 review notes are handled first.
- S2 (assumption) Signals, per configured organization:
  - Review request: an active, non-draft PR where you are a reviewer (not the author) and have not voted, that was not pending in the previous check.
  - Assignment: an open work item assigned to you that was not assigned in the previous check.
  - Comment: a new comment, written by someone else, on an active PR you created or on an open work item assigned to you. Work item comments are only fetched for items whose last change differs from the previous check. PR threads are fetched for at most 25 of your PRs.
- S3 (assumption) Checking: the first check of an organization records the current state without notifying, so there is no initial flood. Seen state persists in the app config dir, so a restart does not replay old notifications. Default interval 5 minutes, adjustable from 1 to 60. An organization that fails (network, expired token, 10 s+ per request batch) is skipped for that round.
- S4 (assumption) Delivery: one native notification per new item, or a single summary notification when a check finds more than 3. Desktop notifications cannot open an item on click (the plugin's actions are mobile only), so every new item is also added to an Inbox in the app (at most 100, newest first, unread until opened or marked read). Unread count shows in the sidebar, as the Dock badge and next to the menu bar icon (macOS).
- S5 (assumption) Settings > Notifications: on/off, interval (minutes), and one switch per signal (review requests, assignments, comments). Stored in `settings.json`; files without the field load the defaults.
- S6 (assumption, security) All requests run in Rust with the stored PATs, no new PAT scope; notification and Inbox text is plain text (titles and names only, no comment bodies); the Inbox file holds no credentials.
- S7 Phase 2 review notes (from L8 of `odd/tasks/global-search-bar.md`): a failing menu bar icon must not stop the app from starting (and closing the main window then quits instead of hiding); a global shortcut that could not be registered at startup is shown in Settings > Shortcuts.

## Tasks
- T0 [S7] Phase 2 follow-ups. Route: inline. Risk: none. Status: done, see L3.
- T1 [S2, S3, S4] Pure detection rules + Inbox/seen-state store with tests. Route: inline. Risk: none (new files; `WorkItem`, `PullRequest`, `SearchHit` gain `Deserialize`). Status: done, see L4.
- T2 [S2-S6] Poller, fetchers, native notifications, badges, commands, settings shape. Route: inline. Risk: items 2 (stored PATs of every organization used in the background), 4 (background task and shared state), 5 (`tauri-plugin-notification`). Status: pending.
- T3 [S4, S5] Inbox view, sidebar count, Settings > Notifications. Route: inline. Risk: none expected. Status: pending.
- T4 [S1-S7] Independent verifier (items 2, 4, 5). Status: pending.

## Log
- L1 (2026-11-07) User accepted: "that works" (signals: review requests, new assignments, new comments on your PRs/work items; every 5 minutes, adjustable).
- L2 Evidence: tauri-plugin-notification docs mark Actions/onAction as "Mobile Only", so desktop notifications cannot carry a click target -> Inbox. PR list fetching (`queries::fetch_pull_requests`, role Mine + status Active) returns both authored and reviewing PRs with `role`, `my_vote`, `is_draft`; `AzdoClient::my_work_items` returns open assigned items with `changed_at`; PR threads (`RawThread`/`RawComment` with author id) and work item comments (`7.1-preview.4`, `created_by` identity) already have raw types.
- L3 T0: `install_tray` failure is logged and startup continues; `background::TRAY_READY` gates hide-on-close (without a tray the main window closes as before). Startup and each successful settings save record the shortcut state; new command `global_shortcut_problem` feeds a notice in Settings > Shortcuts ("That shortcut could not be registered; another app may be using it. Record another shortcut, or use Search… in the menu bar icon."). No unit test (GUI/plugin wiring); cargo test 164 + 5 ok, clippy clean, tsc + build ok.
- L4 T1: `notifications/rules.rs` (`is_review_request`, `authored_to_check`, `work_items_to_check`, `diff` with baseline-only first check, per-target comment seeding, pruning of items no longer current) and `notifications/store.rs` (Inbox text, merge capped at 100, mark read, summary above 3, JSON load/save via `settings::write_atomic`). RED 7/7 and 6/6 (`todo!` stubs) then GREEN 13/13; one GREEN fix (title cap applied after the prefix). Dead-code warnings until T2 wires the poller.
