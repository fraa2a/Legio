# Phase 02: Local state, settings, and library

## Status

Planned

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

- Choose the SQLite crate and migration mechanism at 02A based on current needs and Tauri compatibility.
- Define the first settings set immediately before 02B to avoid speculative columns.

## Update notes

No implementation updates yet.
