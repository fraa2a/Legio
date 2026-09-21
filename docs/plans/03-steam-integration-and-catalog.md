# Phase 03: Steam integration and catalog

## Status

Planned

## Canonical requirements

Owns `PLAN.md` sections 5, 7, 19, 30, 31, and 33.

## Objective

Detect local Steam installations and games, maintain a searchable local catalog, and establish bounded networking, caching, connectivity, cancellation, and diagnostics before remote catalog use.

## Scope

Centralized HTTP behavior, local logs, Steam VDF and ACF discovery, imported library entries, separately indexed Steam catalog data, and on-demand metadata and artwork caching.

## Milestones and principal tasks

### 03A: Networking, connectivity, cache, and diagnostics

- Build one Rust HTTP client with timeouts, cancellation, bounded responses, and typed errors.
- Track connectivity without aggressive retry loops.
- Store cache metadata and actionable local logs without secrets.

Verification: timeout, cancellation, offline, oversized response, cache hit, and cache miss scenarios produce bounded, typed, diagnosable outcomes.

### 03B: Steam installation and library detection

- Discover supported Steam locations on Windows and Linux.
- Parse VDF and ACF metadata as untrusted input.
- Import installed Steam games using the Phase 02 identity model.
- Preserve later launch through Steam rather than direct executable substitution.

Verification: fixture and local-install scenarios discover libraries and imports deterministically while malformed metadata reports useful errors.

### 03C: Steam catalog and media cache

- Keep the catalog index separate from the user library.
- Index search data locally so typing never depends on a live request.
- Fetch metadata and artwork on demand through the shared client.
- Apply bounded storage and cache metadata rules.

Verification: local search works offline after indexing, uncached details load on demand, and stale or unavailable artwork degrades clearly.

## Dependencies

Phase 02 supplies persistent state, settings, game identity, and local library operations.

## Completion criteria

Steam installations and games are detected, imports retain Steam ownership, catalog search is local while typing, media is cached on demand, and network failures are bounded and diagnosable.

## Open technical decisions

- Select the authoritative Steam catalog data endpoint and document its terms before 03C.
- Define cache limits and refresh policy using measured payload sizes before implementation.

## Update notes

No implementation updates yet.
