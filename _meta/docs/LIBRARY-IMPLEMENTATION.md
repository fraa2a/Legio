# Library and playtime implementation

## Agreed scope

Use readable game directories without UUID wrappers. Preserve a valid recorded installation root and every file under it when transferring. Keep executable names and relative paths. Preserve the original game directory name on transfers. Explicitly retain legacy update layouts.

Each library owns one readable JSON library.ldf. It contains a stable library ID, game IDs, relative installation paths, Steam identity and optional update metadata. Only the SHA-256 of the validated source URL is exported. Source URLs, trust decisions and preferences stay local. Source hashes map to existing local source IDs through an indexed database column.

Register library directories centrally in SQLite. The manifest is portable installation metadata; database records are the operational index. Import validates and restores a manifest into that index. An outbox records changes before manifest publication, so a crash cannot resurrect deleted games. Successful database changes do not become failed commands because manifest publication needs retry. A corrupt or unavailable library is isolated and preserved, without blocking unrelated startup operations.

Serialize the full manifest read/modify/write cycle with filesystem locks, and write flushed temporary files with platform-specific atomic replacement. Do not write manifests into arbitrary parents of manual installations. Preserve existing TOML manifests through explicit migration. Keep per-operation journals for file/database recovery.

Advanced playtime editing changes only Legio's total via a persisted adjustment, preserving actual sessions and calendar activity. Reject invalid totals and edits while a game/session is active. Show hours/minutes, an explicit save action and a warning in the danger section. No Steam account data is changed.

## Implementation plan

Native execution on the existing PR branch, without subagents. No version bump, no rendering dependencies, no merge while checks fail.

- [ ] Add regression coverage for recorded roots, folder names and source hashing; correct transfer/manual executable selection and legacy update layout metadata.
- [ ] Add schema migration for registered libraries, pending manifest writes, update source hashes and playtime adjustments. Exercise fresh and existing database migration paths.
- [ ] Implement versioned JSON manifests, validation, local source matching, import and per-library synchronization. Test corrupt files, removals, missing sources, concurrent publication and portable paths.
- [ ] Route startup and mutating commands through recoverable synchronization, remove the duplicate global catalog, and expose library registration/import through the existing locations UI.
- [ ] Add playtime editing service, command and danger panel. Test increase/decrease, active-session rejection and frontend refresh.
- [ ] Run frontend checks/tests/build and native verification where available; push progressive commits, inspect Linux/Windows CI, self-review the complete diff and update PR #201.

## Review focus

- Imported paths must not escape the library or cross links/reparse points.
- Pending manifest writes must win over stale manifests after a restart.
- An unavailable disk must not prevent unrelated games from launching.
- Source hashes never grant trust and source URL changes never silently rebind a game.
- Transfers must preserve recorded layout, working directories and all sibling game resources.

## Execution record

- Existing feature checkout is clean and isolated from main. Updated from origin/main before implementation.
- User explicitly requested implementation after the in-chat design and review; continue within that authorization.
- Rust tooling is absent locally; investigate runtime availability, otherwise use native CI and report the limitation explicitly.
