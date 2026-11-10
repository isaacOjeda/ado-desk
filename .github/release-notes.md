ADO Desk: a desktop app for Azure Boards work items and Azure Repos pull requests across one or more Azure DevOps organizations.

## Download

| Platform | File |
| --- | --- |
| macOS, Apple Silicon (M1 and later) | `ADO.Desk_<version>_aarch64.dmg` |
| macOS, Intel | `ADO.Desk_<version>_x64.dmg` |
| Windows 10/11, x64 | `ADO.Desk_<version>_x64-setup.exe` |

## Install

The builds are not signed with a paid certificate, so each OS warns on first launch.

**macOS**

1. Open the `.dmg` and drag **ADO Desk** into **Applications**.
2. Remove the quarantine flag once, otherwise macOS reports the app as "damaged" or "cannot be verified":
   ```bash
   xattr -cr "/Applications/ADO Desk.app"
   ```
3. On first launch, choose **Always Allow** when macOS asks for keychain access.

**Windows**

Run the installer. If SmartScreen shows "Windows protected your PC", choose **More info** → **Run anyway**.

## Sign in

Use a personal access token with these scopes: Work Items (Read), Code (Read), Project and Team (Read).
