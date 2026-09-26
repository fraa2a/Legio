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

Status: Queue backend implemented; bandwidth limit now persists and restores at startup. Frontend controls and manual restart verification remain.

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

Status: Backend implementation, including a staged executable scan returning relative candidate paths, is in place. Frontend executable confirmation and manual conflict recovery remain.

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

- Define frontend handling for irrecoverable finalization conflicts and stage cleanup errors.

## Update notes

05B backend: `archive_install::verify_and_stage` verifies SHA256 before creating staging content. A mismatched queue-owned download is deleted. ZIP, 7z, and RAR are read through libarchive. The extractor accepts only regular files and directories, streams data under entry-count and expanded-byte limits, and removes incomplete staging directories after errors. Disk exhaustion returns retry guidance. The staging path becomes usable only after extraction succeeds.

The `stage_download` command takes a 05A download ID, verifies the queue-owned archive, and persists `staging` then `staged` with a deterministic staging path in schema v8. Startup removes interrupted staging content and returns that job to `downloaded` or `queued`. A hash or extraction failure records `failed`; a database write failure removes newly extracted content and leaves `staging` for startup recovery.

Encrypted archives, multipart archives, self-extracting archives, links, special files, and formats outside ZIP, 7z, and RAR are unsupported. RAR variants unsupported by libarchive return an extraction error. Linux builds need libarchive development headers and a runtime library; Windows builds use a statically linked vcpkg libarchive package.

05C backend: executable scanning ranks candidates by game name and directory location, filters common installers and helpers, and returns choices when ranking is ambiguous. Manual import validates a user-selected executable and stores its path in a local library entry. The selected path can be changed later. Manual import UI remains open; native Windows launch is tracked in Phase 06B.

05D backend: `scan_staged_executables` accepts a download ID, checks that its status is `staged` and its stored path matches the app-owned staging directory, then returns relative executable candidates. `finalize_download` accepts one selected relative path and repeats containment, file-type, and symlink checks before installation. Schema v10 stores the finalization intent before copying into an app-owned `installed/<download-id>` directory. The copy uses a token-marked sibling temporary directory and a no-replace atomic rename. Linux uses `renameat2` with `RENAME_NOREPLACE`; Windows uses `MoveFileW`. The game entry and `installed` download state commit in one SQLite transaction. Startup resumes interrupted copies or an already-published install, then retries staged-file cleanup after commit. Conflicting existing paths are preserved with an error on the download. The frontend still needs to confirm the candidate and present recovery guidance for conflicts.

05D path contract: staged executable candidates use forward slashes on every platform, matching the finalizer's portable relative-path validation. A regression test scans a staged fixture and finalizes using the returned candidate without frontend path conversion.

05A backend follow-up (2026-09-24): `set_download_bandwidth_limit` now stores bytes per second in the existing SQLite settings table and applies the value to the live queue only after persistence succeeds. Startup reads the saved value before starting queue recovery. Zero continues to mean unlimited. Rust database coverage checks the default, persistence across reopen, clearing to unlimited, and rejection above `i64::MAX`; the local workspace used for this change does not have the Rust toolchain installed, so that test still needs to run in CI.
