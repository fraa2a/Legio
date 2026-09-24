# Phase 02: Local state, settings, and library

## Status

Completed

## Canonical requirements

Owns `PLAN.md` sections 6 and 29, plus persistent settings and local-library portions of section 4.

## Objective

Create the smallest durable local data model needed for settings and a user-controlled game library.

## Scope

SQLite, migrations, typed settings, settings UI, stable game identity, manual metadata overrides, and initial local library operations. Add schema only when a milestone requires it.

## Milestones and principal tasks

### 02A: SQLite initialization and versioned migrations

- Initialize the database in Rust with an explicit location and lifecycle.
- Add transactional, versioned migrations only for current needs.
- Surface migration and corruption failures with useful diagnostics.

Verification: a clean launch creates the expected schema, an existing database migrates once, and a failing migration leaves recoverable state.

### 02B: Typed persistent settings

- Define typed settings with validated boundaries and documented defaults.
- Persist changes atomically through thin Tauri commands.
- Build settings controls for the implemented values only.

Verification: settings survive restart, invalid values are rejected, and defaults are observable on a fresh profile.

### 02C: Stable game identity and local library

- Give every game a stable internal ID and optional Steam App ID.
- Enrich records without replacing user-owned identity or edits.
- Persist durable manual metadata overrides.
- Add initial create, read, update, and remove library operations.

Verification: manual and Steam-linked entries survive restart, enrichment preserves overrides, and removal affects only the selected local entry.

## Dependencies

Phase 01 must provide the application shell, command boundary, toolchains, and mandatory checks.

## Completion criteria

The database migrates safely, settings persist through typed boundaries, and local games have stable identity with optional Steam association and durable user overrides.

## Open technical decisions

None.

## Update notes

- 2026-09-21: Phase completed. Rust owns SQLite initialization at the Tauri application-data path through one managed connection. Schema version 1 stores the current theme setting and local-library records only.
- 2026-09-21: Selected bundled rusqlite with transactional PRAGMA user_version migrations and Rust-generated UUID game IDs. Theme is the first implemented setting and applies to the verification surface. The other canonical settings remain deferred until their consumers exist.
- 2026-09-21: Added functional settings and library API testing controls. The native window initialized its local database, and automated persistence tests proved restart retention, override-preserving enrichment, rejected invalid updates, and unsupported-schema recovery.
- 2026-09-24: Extended the typed local settings store with the download bandwidth limit. It defaults to zero (unlimited), persists through the existing settings key/value table, rejects values above SQLite's supported integer range, and is restored into the Rust download queue at startup. This is backend-only; the existing testing surface has not been changed.

### Milestone evidence

#### 02A

- **Platform:** Linux x86_64 native Tauri runtime and Rust tests.
- **Command or scenario:** launch `corepack pnpm tauri dev`; verify the application database at the resolved application-data path; run `cargo test --manifest-path src-tauri/Cargo.toml`.
- **Observable result:** the native Legio window initialized `legio.sqlite3`. Fresh and reopened profiles completed version 1 initialization once, while a newer unsupported schema remained intact and reported an actionable error.
- **Remaining blockers:** None.

#### 02B

- **Platform:** Linux x86_64 native Tauri runtime and frontend checks.
- **Command or scenario:** launch `corepack pnpm tauri dev`; run `corepack pnpm check`, `corepack pnpm lint`, and `corepack pnpm build`.
- **Observable result:** the native runtime initialized its persisted settings store. The testing surface includes only the typed `system`, `dark`, and `light` Theme control, and all frontend checks passed with zero warnings.
- **Remaining blockers:** None.

#### 02C

- **Platform:** Rust persistence tests and the Linux native verification window.
- **Command or scenario:** create, reopen, enrich, update, and remove local records through the Rust database API; run `cargo test --manifest-path src-tauri/Cargo.toml`.
- **Observable result:** all three Rust persistence tests passed. Every record received a Rust-generated stable UUID, optional Steam App IDs persisted, a manual name override outlasted enrichment, invalid nameless updates were rejected, and removal targets only the selected UUID.
- **Remaining blockers:** None.
