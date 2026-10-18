# Launcher review resolution

Review date: 2026-10-18. All 22 findings have corresponding changes. Runtime limits and remaining verification are recorded below; no performance gain is claimed without measurement.

## Functional, performance and Linux compatibility

PR: https://github.com/fraa2a/Legio/pull/211

| Finding | Resolution |
| --- | --- |
| F01 | Quoted Windows arguments preserve backslashes, UNC paths, empty values and trailing separators; formatting round trips through parsing. |
| F02 | Extraction no longer has the fixed 64 GiB ceiling. Expansion ratio, entry limits, paths and integrity protections remain. |
| F03 | Concurrent updater checks and installs share pending work. Installation waits for an ongoing check and prevents replacement of its update. |
| F04 | Superseded and successfully installed updater resources are closed. Cleanup failure is surfaced without preventing relaunch. |
| F05 | Cancelled artwork work cannot start fallback downloads after a late failure. |
| F06 | Completed requests without cached metadata release their request-map entries; obsolete work cannot delete newer pending work. |
| F07 | Game transfers preserve permissions on the installation root as well as its contents. |
| F08 | Installed-folder programs use the sanitized external launch environment, including AppImage library cleanup. |
| F09 | File-manager discovery uses bounded output and a timeout, including subprocesses which retain inherited pipes. |
| F10 | Uninstall reserves only the affected game and releases the global operation lock before filesystem work. |
| F11 | Uninstall intent is persisted before deletion, can be retried after partial failure, and blocks launch of an incomplete installation. Post-removal cleanup issues return warnings separately. |

An additional regression exposed a second artwork-cache lookup which ignored CacheOrFetch after metadata recovery. It now obeys the same policy as the first lookup. The previously failing recovery test passes.

## UI consistency

PR: https://github.com/fraa2a/Legio/pull/212

| Finding | Resolution |
| --- | --- |
| U01 | ErrorBanner renders a persistent accessible message and its optional retry action. |
| U02 | Successful transfer and shortcut warnings are separate from failures and use warning feedback. Uninstall returns cleanup warnings explicitly. |
| U03 | App and game settings share category navigation and close controls, with a valid fallback selection. |
| U04 | Shared resettable toggles and multiline TextField controls replace repeated compositions. |
| U05 | Download behavior, integrity, bandwidth and notifications live under Download. Folders, registered libraries, Steam synchronization and sources live under Libraries and sources. |
| U06 | Semantic success, warning and error colors support dark and light palettes. |

Chromium screenshots render the actual settings dialog with native IPC stubbed. They verify layout and feedback in both palettes, not complete desktop integration.

## Maintainability and correctness

PR: https://github.com/fraa2a/Legio/pull/213

| Finding | Resolution |
| --- | --- |
| M01 | Environment maps use a null prototype, retain __proto__ as data and reject duplicate keys. |
| M02 | One typed download-state definition owns descriptions, actions, grouping, ordering and tones. Context-specific labels remain distinct. |
| M03 | Database migrations and session queries have focused modules; Linux compatibility command preparation is separate from lifecycle bookkeeping. Existing API boundaries and tests remain. |
| M04 | Manifest errors retain the primary failure alongside any temporary-file cleanup error. |
| M05 | Compatibility, per-game, appearance and bandwidth settings share single-flight save coordination and final flush behavior. Each owner retains its validation and draft semantics. |

## Download policy and PR 208 compatibility

PR 208 was reviewed before changing download validation: https://github.com/fraa2a/Legio/pull/208

- Exact source sizeBytes permits an inclusive range from 97% to 103%, with overflow-safe integer comparisons.
- The original published value is immutable for the queued job; updating exact host length cannot compound the allowance on retries.
- Content length, resumed range totals, received bytes and SHA-256 validation remain strict for the selected archive.
- Hydra's human-readable fileSize is not treated as an exact byte count. Without sizeBytes, the validated exact host size remains authoritative.
- Candidate order, three retries per candidate, manual candidate preference, persisted mirror state and clearing partial bytes on mirror changes are retained.
- The native CDN path remains available. Archive expansion protection is separate from compressed download size.

## Verification

- Functional frontend: 161 tests passed; Svelte check and ESLint passed.
- UI frontend: 158 tests passed; Svelte check, ESLint and production build passed.
- Maintainability frontend: 163 tests passed; Svelte check, ESLint and production build passed.
- Native functional checks: 552 passed, 4 ignored, 15 filtered in the environment-compatible run; all 39 non-live artwork tests passed separately.
- Native maintainability checks: 559 passed, 4 ignored, 8 filtered. After extracting session queries, all 42 database tests and Clippy passed again.
- Clippy with warnings denied passed for both Rust branches.
- Eight process tests cannot run reliably in this container because child PIDs and /proc refer to different namespaces. The four game_process failures also reproduce on unchanged main at a59b93b52260e5fc204245521f29b24d21ef93e9. The remaining process tests pass in the maintainability run.
- Combined-branch validation passed: all 163 frontend tests, Svelte check, ESLint and production build, with no remaining merge conflicts between the three branches.
- GitHub Actions runs the complete suites and Linux/Windows Tauri smoke builds. Check the PR checks for their current outcome.

No release version was changed. These changes do not substitute for testing an installed AppImage and Windows build with actual games, desktop handlers and updater servers.
