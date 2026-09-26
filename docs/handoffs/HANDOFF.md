# Legio handoff

Updated: 2026-09-25

## Current state

- PR #35, `feat: persist Linux compatibility configuration`, was squash-merged to `main` as `d8935f6`. It adds schema v11 compatibility defaults and per-game overrides, effective config resolution, validation, and configured Proton/Wine launching for manually imported Windows games.
- PR #37 was merged to `main` as a docs-only change. The maintained UI checklist and implementation guide are [TODO.md](../frontend/TODO.md) and [backend-contracts.md](../frontend/backend-contracts.md).
- PR #36, `feat: overhaul UI with custom window chrome and component system`, is still open and must not be merged without the user's direction. It currently provides the UI shell and primitives, not functional product pages. Its branch also contains copies of the frontend docs.
- PR #34 delivered Linux runner discovery and basic launch. A real game launch/stop smoke test is still outstanding.
- PR #39, `feat: persist play sessions and playtime summaries`, was squash-merged to `main` as `cc3f42e`. It adds schema v12, process-backed sessions, a 30-second heartbeat, crash recovery, and `get_playtime_summaries`. Its final Rust, frontend, Linux, and Windows checks passed. The Windows test-loader failure was resolved by keeping session tracking independent of the Tauri app handle.
- PR #43, `feat: launch native Windows games with per-game configuration`, was squash-merged to `main` as `444fb10`. It adds schema v13 native arguments and working directory, process ancestry tracking, and Windows launch/stop commands. Frontend, Rust, Linux, and Windows CI passed. A real Windows game launch and stop remains to be observed.
- The current local checkout is `feat/steam-catalog` and dirty. It contains pre-existing changes and untracked files, including local copies under `docs/frontend/` and a modified `docs/plans/README.md`. Inspect `git status` before editing; do not reset, clean, or overwrite unrelated work. Prefer a fresh worktree from current `main` for new backend work.

## Product work remaining

### Backend and verification

- **05A to 05D:** queue, safe archive extraction, executable selection, and staged installation/finalization are implemented. `scan_staged_executables` returns download-bound relative candidates for finalization. Remaining work is end-to-end restart/recovery and cross-platform verification, plus exposing the flows in the UI.
- **06B:** native Windows game launching and helper-to-game process tracking are in `main`. CI passed; a real launch/stop scenario remains to verify before completion.
- **06C:** connect the persisted playtime summaries to Home/Library and finish offline behavior. Session persistence is in `main`.
- **07A:** run a supported Windows game through an installed Proton or Wine runner and record observable launch and stop results.
- **07C:** compare OFLL behavior against an exact revision and record a capability matrix, supported behavior, and explicit exclusions.
- **08:** the UI implementation remains in PR #36. Follow [TODO.md](../frontend/TODO.md), and keep backend contracts aligned with [backend-contracts.md](../frontend/backend-contracts.md). The product mockup requirement in section 35 may still block final UI acceptance; verify its current status.
- **09A to 09C:** define game update and rollback behavior, launcher update trust/restart behavior, release packages/checksums/signing, and install acceptance tests.

### Current frontend/backend contracts to preserve

- Hydra search goes through Rust: `search_catalog` reads local cache; `refresh_catalog` makes the remote Hydra request and caches validated Steam results. The UI must preserve cached results if refresh fails.
- The Legio manifest is a separate source from Hydra. Join it to catalog records by `steamAppId`; only queue downloads through the backend and require confirmation for unverified releases.
- Steam rescan preview is `scan_steam_installations`; the separate `import_steam_installations` command rescans and mutates the library. Reload `list_games` after import.
- Manual executable import needs a native picker, `scan_game_executables`, explicit candidate selection when ambiguous, and `import_manual_game`. Updating a manual game's executable uses `set_game_executable`.
- Downloads have no push progress events; the UI polls `list_downloads`. Launch states are `idle`, `launching`, and `running`; cancellation pending is a local presentation state.
- `get_playtime_summaries` returns `gameId`, `totalMilliseconds`, and `activeSessions` from persistent sessions.
- Steam account overrides use saved Steam IDs and display names. Never assume an `unknown` account match. Confirm before switching accounts when Steam is running.
- Compatibility settings are global defaults plus nullable per-game overrides. Keep inheritance semantics and Linux-only manual-game launch constraints intact.

## Suggested next session workflow

1. Check `git status`, current branch, and the latest GitHub PR/CI state using the GitHub connector. Do not assume this checkout is clean or that PR #36 has merged.
2. Read `AGENTS.md`, `PLAN.md`, this handoff, the relevant `docs/plans/06-launch-playtime-and-offline.md` and `docs/plans/07-linux-compatibility-and-ofll.md`, then the frontend guide if changing command contracts.
3. Start backend work in a fresh worktree from current `main`. Keep Rust as owner of processes, files, downloads, installation, validation, and persistent state. Keep Tauri commands thin and Svelte out of backend decisions.
4. Pick one bounded milestone. After the native Windows launch PR, prioritize Phase 05 end-to-end restart/recovery checks and Phase 07A real-game runner verification. Keep platform smoke tests visibly open until recorded.
5. For database changes, add a forward-only schema migration and migration tests that preserve existing records. Do not add migration from an old application data directory; the database migration feature itself remains required.
6. Add tests for behavior and failure cases, then run `cargo fmt --check`, relevant Rust tests, `cargo clippy --lib --locked -- -D warnings`, and platform CI. The sandbox previously denied loopback socket binds for existing HTTP tests; report that limitation and use CI results rather than changing unrelated tests.
7. Update the owning phase document with status and evidence. Open a focused PR through the GitHub connector, attach it to the task, and do not merge until required checks pass unless the user explicitly directs otherwise.

## Guardrails and decisions

- The user asked for backend readiness while UI remains under active development. Avoid implementing new UI unless asked.
- Do not remove database migrations. There is no need for a one-time migration from a pre-release legacy directory.
- Do not bump the application version unless a release is explicitly requested.
- Use GitHub tools instead of GitHub CLI for repository operations.
- Keep TODO/guide updates in the same change whenever a backend command, serialized field, enum, or state transition changes.
- Achievements and optional/general account features remain deferred pending an explicit product decision.
