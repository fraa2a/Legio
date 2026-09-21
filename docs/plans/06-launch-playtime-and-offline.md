# Phase 06: Launch, playtime, and offline

## Status

Planned

## Canonical requirements

Owns `PLAN.md` sections 23, 24, and 27, plus Home and local Library behavior from section 4.

## Objective

Provide one reliable launch lifecycle across ownership types, persist actual play sessions, and preserve local product behavior without network access.

## Scope

Shared launch lifecycle, Steam-managed launch, native Windows process tracking, session storage and aggregation, Home, local Library, launch configuration and errors, Retry, and offline waiting semantics.

## Milestones and principal tasks

### 06A: Shared launch lifecycle

- Define Rust-owned `prepare`, `launch`, `monitor`, `terminate`, and `collect diagnostics` stages.
- Represent stage failures as typed, actionable results.
- Keep platform-specific behavior behind the shared lifecycle only when needed.

Verification: a controlled executable traverses every stage, termination is observed, and stage failures expose useful diagnostics.

### 06B: Steam and native Windows launch

- Launch Steam-managed games through Steam.
- Launch native Windows games with structured arguments and working directory.
- Track actual game lifetime rather than short-lived helpers.

Verification: Steam ownership remains intact and helper-exit scenarios do not prematurely end monitoring.

### 06C: Sessions, product surfaces, and offline behavior

- Persist sessions and derive playtime aggregates.
- Populate Home and local Library from stored data.
- Expose launch configuration, errors, and real Retry behavior.
- Keep network tasks waiting rather than retrying in a loop.
- Preserve Home, Library, local metadata, playtime, settings, and permitted installed-game launch offline.
- Mark remote Store, source, download, and update work unavailable offline.

Verification: session totals survive restart, local surfaces remain useful offline, permitted games launch, and remote work waits without request churn.

## Dependencies

Phase 05 supplies installed game records and executable selection. Phase 03 supplies shared connectivity state.

## Completion criteria

Supported games launch through the correct owner, actual game lifetime drives sessions, playtime persists, local product surfaces work offline, and remote operations wait clearly.

## Open technical decisions

- Define helper-to-game process association rules before 06B.
- Define session crash recovery and overlap rules before 06C.

## Update notes

No implementation updates yet.
