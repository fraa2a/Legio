# TaxLauncher Replacement Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Deliver a recoverable Windows bridge update that imports TaxLauncher data into Legio before removing TaxLauncher.

**Architecture:** Native Legio headless import owns validation and the SQLite transaction. An Electron bridge exports legacy data and hands replacement to a detached PowerShell helper. The bridge build contains a pinned installer and validated source snapshot.

**Tech Stack:** Existing Rust/SQLite/Serde, Electron, Node built-ins, Windows PowerShell 5.1 and NSIS.

**Spec:** docs/superpowers/specs/2026-10-11-taxlauncher-replacement.md

## Global Constraints

- Windows only; use production Tauri application data resolution.
- TaxLauncher source reference directory remains unchanged; isolated bridge copy is used.
- Never remove games or legacy user data; uninstall only after a successful native import receipt.
- Source: https://source.taxphobia.top/store.json; both logging toggles remain off by default.
- Do not merge PR 183 or publish production feeds/releases.

## Review Focus

- Invalid/partially written input must leave the database unchanged.
- Retrying after interruption must not duplicate games or playtime.
- Existing Legio preferences/games/source must survive import.
- Installer/import/uninstall failures must remain recoverable with a transcript.
- Legacy paths inside the uninstalled application must prevent replacement.

### Task 1: Native transactional importer

**Files:** Create src-tauri/src/taxlauncher_import.rs; modify src-tauri/src/lib.rs and main.rs.
**Interfaces:** run_cli(args: &[OsString]) -> Option<Result<(), String>>; import(database: &Database, input: &Export) -> Result<Receipt, String>.

- [x] Write failing fixture tests for games/path/minutes/settings/source conversion, repeat import, existing Legio settings, malformed input, transaction rollback and invalid cover identity.
- [x] Run focused native tests and record RED.
- [x] Implement bounded export parsing, native transactional import and CLI receipt export before GUI startup; validate bundled source with parse_manifest.
- [x] Run focused tests, format and Clippy; record GREEN and commit native changes.

### Task 2: Windows replacement bridge

**Files:** In taxlauncher-legio-migration create bridge-main.js, migration/export.js, migration/replace.ps1, migration UI/preload, preparation/build scripts and tests. Modify package.json and installer.nsh.
**Interfaces:** prepareExport(userData, preferences, installDir, manifest) -> exportPath; detached helper consumes a JSON plan with exact pinned files and paths.

- [x] Write failing Node tests for real temporary-directory export, legacy preference allowlist, backup, unsafe paths, incomplete downloads and pinned payload validation.
- [x] Run Node tests and record RED.
- [x] Implement export and bridge UI, install/import/uninstall helper, and fail-closed build preparation. Exclude secrets and legacy publishing tooling from distributed files.
- [x] Run Node tests and parse PowerShell if a runtime is available; document platform limitations and commit local bridge changes.

### Task 3: Integration verification and handoff

**Files:** docs/releases/0.2.0-taxlauncher-migration.md and bridge README.md.

- [ ] Run native importer fixtures and relevant checks, bridge suite, inspect package contents and helper failure boundaries.
- [ ] Obtain one fresh independent whole-change review, handle important findings in one test-backed fix pass.
- [ ] Push native importer to draft release PR, obtain Windows CI artifacts and matching provenance, prepare bridge payload from that tested installer.
- [ ] Build a Windows bridge artifact if the available packaging toolchain supports it; otherwise preserve runnable sources and state the concrete limitation.
- [ ] Deliver Windows acceptance steps, recovery paths and publication sequencing without publishing production feeds.
