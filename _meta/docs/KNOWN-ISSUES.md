# Known issues and limitations

Track unresolved non-security issues here; use linked GitHub issues and PRs for discussion. Set status to `Open`, `In progress` or `Blocked`, name an owner only after they accept, and explain blockers. Remove entries when their fix merges. Report vulnerabilities through [SECURITY.md](../../SECURITY.md).

## KI-001: Unix-only test runs on Windows

**Defect | Open | Unassigned | Issue/PR: not linked**

`src-tauri/src/launch_environment.rs` gates tests with `cfg(test)` but uses Unix paths and `/bin/sh`. This affects Windows native tests, not necessarily game launching.

**Fix:** gate Unix-specific assertions, retain meaningful cross-platform coverage, and run the affected tests on Linux and Windows.

## KI-002: Transfers reject symbolic links

**Compatibility limitation | Open | Unassigned | Issue/PR: not linked**

`src-tauri/src/game_transfer.rs::copy_contents` rejects symbolic links, including legitimate links in native Linux installations.

**Workaround:** manage these installations outside Legio's transfer action. Do not bypass link validation.

**Resolution:** define support for contained relative links. Before relaxing rejection, test escaping, dangling and cyclic links, permissions and interrupted transfers.
