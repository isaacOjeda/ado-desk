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
```

- Work items: WIQL `[System.AssignedTo] = @Me` excluding Closed/Done/Removed, up to 200.
- Pull requests: active PRs per project where you are the creator or a reviewer.

## Tests

```bash
cd src-tauri && cargo test
```

## Next steps

- Replace PAT with Microsoft Entra ID OAuth (Azure DevOps OAuth is deprecated).
- Pagination and caching; PR details and work item detail panes.
