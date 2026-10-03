# Final review fixes

Baseline: `c0b104157c98f2a767e0f262241ad8576a7235d9` (0.1.3).
Scope: all 21 numbered findings in LEGIO_FINAL_REVIEW plus the associated concrete cleanup requests. The application version is unchanged. Existing migrations remain supported; schema v22 adds persistent installation/transfer metadata and language-specific Steam caches.

| Finding | Result |
| --- | --- |
| 1. Second instance recovery race | Single-instance plugin runs before setup/database recovery. Forwarded shortcut launches use the original instance and main-thread dispatch. |
| 2. Interrupted transfer | Persisted prepared/copied/committed intent. Copy and sync before no-replace publication; original retained through database commit. Startup recovery and owned-directory cleanup. |
| 3. Stale launch paths | One transaction remaps the executable, installation root, installed download path, native/compatibility working directories and compatibility prefix inside the source. External paths stay external. |
| 4. Stale Steam response | Monotonic per-App-ID generations, including refresh and language changes. Failed refresh keeps usable cached data. |
| 5. Poll starvation | Concurrent resource reads share one promise. Explicit mutation invalidates older reads. |
| 6. SQLite per chunk | Atomic stop flags and progress writes on the blocking pool, coalesced every 500 ms. |
| 7. Retroactive throttle | Limit changes reset the accounting window. Waits check stop/limit at most every 50 ms. |
| 8. Custom artwork rereads | Shared bounded cache with request deduplication, retained Blob URLs and targeted icon/banner invalidation. |
| 9. Artwork after unmount | Generation/destroy guards and idempotent reference release cover in-flight reads and mutations. |
| 10. UI language and locale | Persisted System/Italian/English preference, reactive UI catalog, localized dialogs/tray, document language and Steam descriptions. Regional numbers/dates use system locale independently. |
| 11. Lost installation root | Root belongs to the game row and survives history cleanup. Migration backfills existing installed entries. OnlineFix detection uses the stored root with bounded traversal. |
| 12. Active game removal | Launch preparation, transfer and removal share the lifecycle operation lock. Active games cannot be removed or transferred; frontend reloads even after later cleanup failure. |
| 13. Repeated PNG processing | Blocking worker pool with two concurrent transforms, decode dimension/allocation bounds, and versioned transformed cache entries. Invalid images fail closed. |
| 14. WebView/IPC hardening | Restrictive CSP, explicit application command manifest/permission and main-window capability. |
| 15. Process-output privacy | Streaming redaction of configured environment values and inherited secret-like environment values, including secrets split across reads. |
| 16. Missing tray | Minimized startup happens only after tray success. Hide-on-launch is enabled only when the backend reports a tray. Window operation failures are logged. |
| 17. Linux autostart | Reuses desktop-entry argument escaping; create-new temporary file, sync and atomic publication. |
| 18. Partial reorder | Status validation and every position update are in one SQLite transaction, with rollback regression coverage. |
| 19. Misleading disk requirement | Compressed bytes are labelled download size; unknown extraction/temporary space is explicit. |
| 20. Unsafe constraints | statvfs and Windows free-space calls document pointer, buffer and initialization preconditions. |
| 21. Release trust | The release calls the complete CI workflow for the release SHA before building/publishing. External Actions are pinned to commits. AUR SSH requires independently verified known-host entries. |

## Concrete cleanup

Removed unused frontend count exports and unused account-check wrapper. Renamed ambiguous migration/OnlineFix tests while preserving their coverage. Unsupported-platform fallback branches were removed after a clear Linux/Windows compile boundary. Settings and shortcut commands delegate to domain services. Repeated launch-field formatting/parsing helpers are shared, runner options have runtime type guards, and the legacy global working directory remains explicitly deprecated. No mass renaming or speculative lifecycle rewrite is included.

## Verification

Frontend checks, lint, build, Home tests, asynchronous store/artwork/localization regressions and release-manifest tests run locally. Rust formatting and Clippy cover all targets/features with warnings denied. Transfer recovery tests reopen the database after publication, inject a database commit failure, verify path rollback and exercise repeated recovery. Queue tests cover transactional reorder failure and preservation of installation metadata after history cleanup. Image and streaming redaction tests cover allocation boundaries and cross-read secrets.

The local focused Rust run passed 278 tests with four intentional ignored tests; process/lifecycle modules are checked separately. Six existing Linux process/lifecycle tests cannot observe child processes correctly in this sandbox's PID namespace. The CI workflow runs the complete unfiltered suite plus Linux/Windows Tauri builds and specific Windows lifecycle/finalization tests. Do not weaken those tests to accommodate the sandbox.

## Deployment configuration and remaining acceptance work

Before the next stable release, set repository variable `AUR_KNOWN_HOSTS` to complete `aur.archlinux.org` OpenSSH known-host entries independently verified against Arch's published SSH fingerprints over a trusted channel. The workflow deliberately fails when the variable is absent or the key changes. A release does not learn SSH trust through `ssh-keyscan` in its own network session. Key rotation requires verification and an explicit variable update.

Proton writes its own log file directly; stdout/stderr streaming redaction does not sanitize arbitrary content Proton or a game writes into that external file. The diagnostic report redacts configured environment values, and log files remain permission restricted and bounded. Values transformed or invented by a game cannot be guaranteed to match the redaction list.

Still requires real-system acceptance: two simultaneous launcher invocations, tray unavailable on a real desktop, transfer across filesystems with forced process termination, long-running throttled downloads, a real native Windows game, Steam/Wine/Proton launches and signed updater/package installation. These are acceptance tasks, not claimed test results. No release or merge is requested by this change.
