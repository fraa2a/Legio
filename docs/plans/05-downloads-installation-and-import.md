# Phase 05: Downloads, installation, and import

## Status

In progress

## Canonical requirements

Owns `PLAN.md` sections 12, 14 through 18, and 34, plus the first usable Downloads controls required by section 4.

## Objective

Download, verify, extract, identify, and finalize supported games through a persistent and recoverable Rust-owned pipeline.

## Scope

Persistent queue control, restart recovery, bandwidth and progress reporting, SHA256 verification, staged safe extraction, executable detection, manual import, cross-filesystem finalization, and atomic library updates.

## Milestones and principal tasks

### 05A: Persistent download queue

Status: Queue backend implemented; bandwidth limit now persists and restores at startup. The Downloads screen shows the queue, controls pause/resume/retry/cancel, and Settings applies the persisted limit through `set_download_bandwidth_limit` with `get_download_bandwidth_limit` for read-back. Manual restart verification remains.

- Implement queue, pause, resume, retry, cancel, and waiting states.
- Use HTTP Range when supported and restart safely when it is not.
- Recover queue state after restart and interruptions.
- Report progress, speed, ETA, and bandwidth-limit effects.

Verification: restart, cancellation, retry, offline waiting, Range support, Range fallback, and interrupted transfer scenarios preserve consistent state.

### 05B: Verification and safe staged extraction

Status: Backend implemented; manual restart and platform verification remain.

- Verify SHA256 before extraction.
- Extract supported ZIP, 7z, and RAR archives into staging.
- Reject traversal, absolute paths, malformed paths, symlink escape, and unsafe expansion.
- Bound archive expansion and handle archive bombs, disk exhaustion, and invalid-hash cleanup or quarantine.
- Exclude encrypted and multipart archives.

Verification: adversarial fixtures cannot escape staging, invalid hashes never install, limits stop unsafe extraction, and recovery guidance is actionable.

### 05C: Executable detection and manual import

Status: Backend implemented; candidate selection and manual import UI remain.

- Rank executable candidates using explicit signals.
- Ask the user when ranking is ambiguous.
- Begin manual import from a user-selected executable.

Verification: clear candidates auto-select, ambiguous candidates require choice, and manual imports create a valid Phase 02 library entry.

### 05D: Atomic finalization

Status: Backend implementation, including a staged executable scan returning relative candidate paths, is in place. The Downloads screen stages a download, requires an explicit candidate when ranking is ambiguous, and finalizes with the selected relative path. Manual conflict recovery and end-to-end verification remain.

- Move staged content safely across filesystems.
- Make filesystem and database finalization recoverable as one user-visible operation.
- Update the library only after verified installation is usable.
- Provide manual recovery for states that cannot be repaired automatically.

Verification: same-filesystem, cross-filesystem, disk-full, database-failure, and restart scenarios never claim an unusable installation as complete.

## Dependencies

Phase 04 supplies validated releases, direct URLs, hashes, trust state, and availability behavior.

## Completion criteria

The queue is persistent and controllable, supported archives are verified and extracted safely, executable selection is deterministic or user-resolved, and successful finalization updates the library atomically.

## Open technical decisions

- None open for the frontend flow. Irrecoverable finalization conflicts and stage cleanup errors are presented per status, as recorded in the Downloads UI follow-up note below.

## Update notes

05B backend: `archive_install::verify_and_stage` verifies SHA256 before creating staging content. A mismatched queue-owned download is deleted. ZIP, 7z, and RAR are read through libarchive. The extractor accepts only regular files and directories, streams data under entry-count and expanded-byte limits, and removes incomplete staging directories after errors. Disk exhaustion returns retry guidance. The staging path becomes usable only after extraction succeeds.

The `stage_download` command takes a 05A download ID, verifies the queue-owned archive, and persists `staging` then `staged` with a deterministic staging path in schema v8. Startup removes interrupted staging content and returns that job to `downloaded` or `queued`. A hash or extraction failure records `failed`; a database write failure removes newly extracted content and leaves `staging` for startup recovery.

Encrypted archives, multipart archives, self-extracting archives, links, special files, and formats outside ZIP, 7z, and RAR are unsupported. RAR variants unsupported by libarchive return an extraction error. Linux builds need libarchive development headers and a runtime library; Windows builds use a statically linked vcpkg libarchive package.

05C backend: executable scanning ranks candidates by game name and directory location, filters common installers and helpers, and returns choices when ranking is ambiguous. Manual import validates a user-selected executable and stores its path in a local library entry. The selected path can be changed later. Manual import UI remains open; native Windows launch is tracked in Phase 06B.

05D backend: `scan_staged_executables` accepts a download ID, checks that its status is `staged` and its stored path matches the app-owned staging directory, then returns relative executable candidates. `finalize_download` accepts one selected relative path and repeats containment, file-type, and symlink checks before installation. Schema v10 stores the finalization intent before copying into an app-owned `installed/<download-id>` directory. The copy uses a token-marked sibling temporary directory and a no-replace atomic rename. Linux uses `renameat2` with `RENAME_NOREPLACE`; Windows uses `MoveFileW`. The game entry and `installed` download state commit in one SQLite transaction. Startup resumes interrupted copies or an already-published install, then retries staged-file cleanup after commit. Conflicting existing paths are preserved with an error on the download. The frontend confirms the candidate and lets the download row carry the backend error verbatim.

05D path contract: staged executable candidates use forward slashes on every platform, matching the finalizer's portable relative-path validation. A regression test scans a staged fixture and finalizes using the returned candidate without frontend path conversion.

05A backend follow-up (2026-09-24): `set_download_bandwidth_limit` now stores bytes per second in the existing SQLite settings table and applies the value to the live queue only after persistence succeeds. Startup reads the saved value before starting queue recovery. Zero continues to mean unlimited. Rust database coverage checks the default, persistence across reopen, clearing to unlimited, and rejection above `i64::MAX`; the local workspace used for this change does not have the Rust toolchain installed, so that test still needs to run in CI.

Queue cleanup follow-up: cancelling a download only rewrote its status, so `cancelled` rows and their partial files accumulated with no way to clear them. `remove_download` now deletes the row and its `.part`, `.archive` and `.stage` content for `cancelled`, `failed` and `installed`, refuses every other status, removes files before the row so a cleanup failure stays visible, reuses the finalizer's staged-directory safety check so unexpected content is never deleted, and leaves `installed/<id>` plus the library entry alone because the library owns the game. `remove_finished_downloads` clears all `cancelled` and `installed` rows while keeping retryable `failed` rows. Rust coverage checks each removable status, refusal of the states that can still progress, preservation of an installed game, retention of `failed` during bulk cleanup, and refusal to delete a symlinked staging directory.

Queue UI copy: a download row shows the state label and, when present, the backend error verbatim. Explanatory lines per state, recovery guidance and result confirmations were removed on purpose, so the copy does not restate what the label, the enabled actions and the error already say.

Downloads UI follow-up: `get_download_bandwidth_limit` exposes the stored value so the Settings control reports the persisted limit after a restart instead of implying it is unlimited. The Downloads screen drives the install flow: `stage_download` on `downloaded`, `scan_staged_executables` on `staged` with an explicit choice when `selectedRelativePath` is null, then `finalize_download` with the chosen `relativePath` unchanged. Candidate `relativePath` values are passed through verbatim, so the frontend performs no path conversion or containment checks. Finalization errors are shown verbatim: a rejection before the intent is stored leaves `staged` with the message on the invocation, a conflict leaves `finalizing` with the staged files and the conflicting install target preserved for inspection and no retry command, and an `installed` row with an error means only stage cleanup failed and is retried on the next start.

Install folder access: `open_installed_folder` opens the app-owned `installed` directory in the platform file manager and is surfaced as a Settings button. It takes no path argument, so the frontend can only ask for the directory Legio owns and never for an arbitrary location. `finalize_install::install_root` is reused instead of re-deriving the path, which also creates the directory when nothing has been installed yet. No new dependency was added: the command spawns the file manager directly with structured arguments, following the existing `steam_process` and `game_process` pattern, because a plugin would grant the frontend broader path-opening permissions than this needs. `xdg-open` is not used on Linux: it can resolve a terminal for directories, and it also falls back to one when no handler is registered. The command instead reads the registered handler with `xdg-mime query default inode/directory`, skips it when it is a terminal, and otherwise uses `gio open` so the user's own file manager still decides. Known file managers are the fallback for desktops where the registered handler is unusable or GIO is missing.
