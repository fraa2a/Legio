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

Status: Backend implemented; frontend controls and manual restart verification remain.

- Implement queue, pause, resume, retry, cancel, and waiting states.
- Use HTTP Range when supported and restart safely when it is not.
- Recover queue state after restart and interruptions.
- Report progress, speed, ETA, and bandwidth-limit effects.

Verification: restart, cancellation, retry, offline waiting, Range support, Range fallback, and interrupted transfer scenarios preserve consistent state.

### 05B: Verification and safe staged extraction

- Verify SHA256 before extraction.
- Extract supported ZIP, 7z, and RAR archives into staging.
- Reject traversal, absolute paths, malformed paths, symlink escape, and unsafe expansion.
- Bound archive expansion and handle archive bombs, disk exhaustion, and invalid-hash cleanup or quarantine.
- Exclude encrypted and multipart archives.

Verification: adversarial fixtures cannot escape staging, invalid hashes never install, limits stop unsafe extraction, and recovery guidance is actionable.

### 05C: Executable detection and manual import

- Rank executable candidates using explicit signals.
- Ask the user when ranking is ambiguous.
- Begin manual import from a user-selected executable.

Verification: clear candidates auto-select, ambiguous candidates require choice, and manual imports create a valid Phase 02 library entry.

### 05D: Atomic finalization

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

- Resolve archive backend selection and redistribution constraints before 05B.
- Define invalid-hash removal versus quarantine policy before 05B.
- Define finalization recovery records before 05D.

## Update notes

No implementation updates yet.
