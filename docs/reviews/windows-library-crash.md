# Windows library crash diagnosis

## Confirmed defect

`LibraryView.svelte` keyed Steam diagnostic rows by their message. The scanner can
return the same message for multiple manifests or libraries, so opening the page
with repeated messages throws Svelte's `each_key_duplicate` error. The defective
loop is present in both release tags `v0.1.2` and `v0.1.3`.

The fix keys diagnostic rows by their position. Repeated warnings remain visible
and no longer prevent the library from rendering. The regression test mounts the
actual library component with repeated warnings, updates both games and warnings,
and verifies that the library remains usable when the warnings disappear. It
failed with `each_key_duplicate` before the fix and passes afterwards.

This reproduces a frontend rendering failure. A native process exit on the
affected Windows machine has not been reproduced and needs separate evidence if
it persists with this fix.

## Steam warning investigation

Steam being installed does not guarantee that Legio can read all its metadata.
The current scanner only checks Steam under `ProgramFiles(x86)` and `ProgramFiles`;
it does not discover a custom client installation through the Windows registry.
Once it finds a client, it follows `steamapps/libraryfolders.vdf` for other game
libraries.

The scanner also requires a verified game type from `appcache/appinfo.vdf`. Missing,
unreadable or unsupported metadata can cause installed games to be skipped. The
message `ignored a Steam app without verified game type` is emitted once per
affected app, which can trigger the confirmed rendering defect. Other repeated
messages include unreadable manifests and unavailable installation directories.

The exact warning text and Steam client path from the affected machine are still
needed to identify which condition produced its original yellow warning. This
fix does not change game classification or Steam discovery.

## Windows data locations

The bundle identifier is `dev.fraa2a.legio`.

| Content | Default location |
| --- | --- |
| SQLite database | `%APPDATA%\dev.fraa2a.legio\legio.sqlite3` |
| Application logs | `%LOCALAPPDATA%\dev.fraa2a.legio\logs` |
| Webview data | `%LOCALAPPDATA%\dev.fraa2a.legio` |

The database uses Tauri's `app_data_dir`, while logs use `app_log_dir`. Looking in
the Local directory and finding logs plus webview data does not imply a missing
database. Tauri documents these defaults at
<https://docs.rs/tauri/latest/tauri/path/struct.PathResolver.html>.

The database is initialized during application setup. A failure there can prevent
startup, so successful access to Home and Settings does not match a simple
missing-database diagnosis. The network diagnostics log does not capture Svelte
rendering exceptions or every local Steam scanner warning.

## Validation

- Regression reproduced before the fix and resolved after it.
- Svelte/TypeScript check, ESLint and production frontend build pass.
- Retest on the affected Windows machine remains necessary, including whether
  the window becomes unusable or the executable actually exits.
- No version bump or release is included in this fix.
