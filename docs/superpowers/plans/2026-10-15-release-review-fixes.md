# Release review fixes implementation plan

**Goal:** Address F01-F09 from the release review in one PR without changing runner defaults or weakening recovery safeguards.

**Architecture:** Keep SQLite operational state and portable JSON manifests. Make invalidation explicit, bound artwork demand, reuse immutable source snapshots, and schedule maintenance only when needed.

**Execution:** Direct implementation in the dedicated `fix/release-review-findings` branch, without subagents. Push and squash merge only after required CI checks pass, as requested.

## Constraints

- Preserve Linux and Windows behavior, existing data and recovery intents.
- No version bump, new product dependencies or speculative runner tweaks.
- Add behavioral regression tests before fixes; retain existing coverage.
- Keep cancellation, cross-profile locks, atomic publication and PID identity checks.

## Tasks

1. Add explicit resource invalidation and use it after mutations and native change events. Test an older pending read and coalesced invalidations.
2. Release offscreen card consumers while preserving outgoing-page transitions and shared cache URLs. Test offscreen eviction and revisit.
3. Complete English keys, remove unused base64 and obsolete artwork parameters, and simplify eager fallbacks and redundant platform attributes.
4. Isolate per-job recovery failures, retain launch protection for affected games, and allow safe retry. Test unavailable storage and unrelated game launches.
5. Throttle artwork disk maintenance without exposing uncommitted replacements to eviction. Test publication and maintenance scheduling.
6. Reuse parsed source snapshots and entry indexes across update batches. Keep parsing outside the SQLite lock and invalidate on source changes.
7. Avoid idle policy work and deduplicate external game checks. Retain pre-mutation safety checks and pending-job progress.
8. Publish only changed library manifests, narrow dirty triggers and preserve explicit repair/import behavior. Test playtime-only changes and unchanged publication.
9. Review the whole diff, run frontend and Rust checks, push, create PR, inspect CI, fix failures and merge after all required checks pass.

## Review focus

- A disconnected download volume must not prevent an unrelated game from launching.
- An older IPC read must not overwrite state after a mutation.
- Releasing artwork demand must not revoke URLs still used during a transition.
- Source removal or refresh must not leave update provenance or cached release indexes stale.
- Manifest no-op detection must not suppress pending changes, external-conflict detection or repair.

## Deliberate boundaries

GPU quality changes, runner defaults, automatic umu ID mapping, new GPU-selection UI and generic lifecycle rewrites require measurements or additional product behavior. This PR fixes the demonstrated findings and applies behavior-preserving simplifications; it does not introduce unverified universal performance presets.
