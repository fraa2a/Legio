# Phase 04: Legio source, Store, and trust

## Status

In progress

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

- The project-controlled source URL is `https://source.taxphobia.top/store.json`. The file has not been published yet; refresh must report unavailability until it exists.
- A cached manifest is fresh for 24 hours. Refresh is explicit until the Store flow is implemented.

## Update notes

04A validates the exact versioned source document, applies a size limit, and retrieves only the fixed HTTPS URL `https://source.taxphobia.top/store.json`. The endpoint may return an availability error until the project publishes `store.json`. Cache retention and catalog merge remain 04B work.

04B stores the validated manifest in a single SQLite row. A failed refresh preserves that row and returns it with a stale warning. Catalog searches report source availability by Steam App ID and keep the catalog title. A missing cache reports unknown availability; an absent ID in a valid cache reports unavailable.

04C adds availability and trust states to the catalog verification UI, including stale source warnings and release details for unverified entries. Installation actions will be added with the Phase 05 download pipeline.
