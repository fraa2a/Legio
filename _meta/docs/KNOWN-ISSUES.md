# Known issues and limitations

Track unresolved non-security issues here; use linked GitHub issues and PRs for discussion. Set status to `Open`, `In progress` or `Blocked`, name an owner only after they accept, and explain blockers. Remove entries when their fix merges. Report vulnerabilities through [SECURITY.md](../../SECURITY.md).

## KI-003: Application smoke test does not run on Windows

**Defect | Open | Unassigned | Issue/PR: not linked**

On Windows the Edge driver fails to create a session: `session not created: DevToolsActivePort file doesn't exist`. The debug application starts and stays running when launched directly, so the process itself is not crashing. The driver matched the installed WebView2 runtime version, and turning DevTools on for the test binary did not change the result. CI therefore runs the smoke test, the screenshots and the performance budget on Linux only.

**Fix:** find why WebView2 does not expose its debugging endpoint to `msedgedriver` for this application, for example by starting it with a remote debugging port and querying it. Then re-enable the Windows smoke step in `.github/workflows/ci.yml` and restore the Edge driver selection step.
