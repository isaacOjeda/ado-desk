# ADO Desk

Proof of concept: a desktop app (Tauri 2 + React + TypeScript) that shows your Azure Boards
work items and Azure Repos pull requests.

## Run

```bash
pnpm install
pnpm tauri dev      # first build compiles the Rust side and takes a few minutes
```

## Sign in

Create a [personal access token](https://learn.microsoft.com/azure/devops/organizations/accounts/use-personal-access-tokens-to-authenticate)
with these scopes:

- **Work Items**: Read
- **Code**: Read
- **Project and Team**: Read (used to discover projects for the pull request list)

Enter your organization (`contoso` or `https://dev.azure.com/contoso`) and the token.
The token is validated against Azure DevOps, then stored in the OS credential store
(macOS Keychain / Windows Credential Manager / Secret Service). It never returns to the webview.

## Quick search

Press **⌘K** (Ctrl+K on Windows/Linux) or click *Search* in the sidebar:

- `#123` opens work item 123, `!45` opens pull request 45, a bare number looks up both.
- Other text (2+ characters) searches titles: work items through WIQL across every project, and
  your own pull requests (author or reviewer, any status), since Azure DevOps has no title filter
  for pull requests. Any pull request still opens by `!id`.
- Every configured organization is searched, the active one first. Opening a result from another
  organization switches to it. An organization that cannot be searched (for example an expired
  token) is named above the results; the others still answer.
- Arrows move, Enter opens in the app, ⌘Enter opens in Azure DevOps, Esc closes. With nothing
  typed, it lists the items you opened recently (stored on this device only).

## Themes

Settings > Appearance offers built-in reading themes for light and dark mode, translucency
(macOS) and a glass slider that applies to the whole window.

**Import from Open VSX**: click *Browse themes*, search, and import a VS Code color theme
extension. Every color theme in it is added to the Light or Dark picker (hover a card to
remove it). Notes:

- This is the only network call outside Azure DevOps. It runs in Rust, only when you open the
  browser or search, and only against `https://open-vsx.org` (downloads may redirect only to Open
  VSX storage, `openvsx.eclipsecontent.org`, over HTTPS). Nothing about your organizations is sent.
- The `.vsix` is read in memory with size caps and never unpacked to disk. Only the theme
  colors are kept, mapped onto the app's tokens; tokens a theme does not define are derived
  or come from the default built-in theme.
- Imported themes are stored in `imported-themes.json` in the app config directory.
  Reimporting an extension replaces its themes.
- Imported themes are not contrast-checked like the built-in ones.

## Architecture

```
src/                      React UI (no network access; CSP allows IPC only)
  lib/api.ts              Typed wrappers over Tauri commands
  features/auth           Login screen
  features/workspace      Sidebar, work item list, pull request list
src-tauri/src/
  commands.rs             Tauri commands: login, restore_session, logout, list_*
  credentials.rs          OS keychain storage
  azdo/client.rs          HTTP adapter for the Azure DevOps REST API 7.1
  azdo/model.rs           Domain types and pure payload mapping (unit tested)
  search.rs               Quick search: query syntax, title WIQL, PR title filter
  themes/                 Open VSX search/import, vsix reader, VS Code color mapping, store
```

- Work items: WIQL `[System.AssignedTo] = @Me` excluding Closed/Done/Removed, up to 200.
- Pull requests: active PRs per project where you are the creator or a reviewer.

## Tests

```bash
cd src-tauri && cargo test
cargo test live_open_vsx -- --ignored   # optional: imports real themes from open-vsx.org
```

## Next steps

- Replace PAT with Microsoft Entra ID OAuth (Azure DevOps OAuth is deprecated).
- Pagination and caching; PR details and work item detail panes.
