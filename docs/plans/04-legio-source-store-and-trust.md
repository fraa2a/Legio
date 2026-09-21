# Phase 04: Legio source, Store, and trust

## Status

Planned

## Canonical requirements

Owns `PLAN.md` sections 8 through 11 and 20, plus Browse and Store availability and trust presentation from section 4.

## Objective

Consume one project-controlled Legio manifest safely, preserve the last valid source, merge availability by Steam App ID, and communicate trust without making a safety guarantee.

## Scope

Versioned manifest parsing, semantic validation, atomic caching, Steam catalog merge, availability states, and verified or unverified trust presentation.

## Milestones and principal tasks

### 04A: Exact manifest parser and validator

- Parse the canonical versioned manifest without permissive field invention.
- Preserve separate top-level `verified` and `unverified` lists.
- Enforce exclusive App ID membership, required release fields, and duplicate or conflict checks.
- Retrieve the source only over HTTPS.
- Allow direct archive URLs over HTTP or HTTPS as specified.

Verification: valid fixtures parse exactly and invalid versions, duplicates, cross-list conflicts, missing release data, or invalid schemes fail with typed diagnostics.

### 04B: Atomic last-valid cache and catalog merge

- Write validated manifests atomically while retaining the prior valid copy on failure.
- Expose freshness and stale-cache state.
- Merge source availability into the Steam catalog by Steam App ID without replacing catalog identity.

Verification: interrupted or invalid refresh retains the prior valid manifest, stale state is visible, and merge results are deterministic.

### 04C: Store availability and trust UI

- Show available, unavailable, stale, verified, and unverified states.
- Use the wording `Verified by Legio Staff` without implying a safety guarantee.
- Present an actionable warning that still permits unverified installation.
- Show explicit `Download unavailable` behavior.

Verification: each trust and availability state renders distinct actions and remote text is not treated as UI instruction.

## Dependencies

Phase 03 provides the catalog, shared HTTP behavior, connectivity, caching conventions, and diagnostics.

## Completion criteria

Only validated manifests become current, the last valid cache survives bad refreshes, source availability merges by App ID, and users see accurate trust and availability states.

## Open technical decisions

- Blocking before 04A: choose and document the project-controlled HTTPS source URL.
- Define manifest cache age and refresh triggers before 04B.

## Update notes

No implementation updates yet.
