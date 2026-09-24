# Phase 07: Linux compatibility and OFLL

## Status

In Progress

## Canonical requirements

Owns `PLAN.md` sections 3, 25, 26, and 28.

## Objective

Run supported Windows games on Linux through explicit compatibility configuration and reproduce the required Online Fix Linux Launcher behavior in Rust.

## Scope

Runner discovery and management, prefixes and runtime configuration, launch options, process handling, diagnostics, and an evidence-backed responsibility parity matrix.

## Milestones and principal tasks

### 07A: Runner discovery and basic launch

Status: Proton, GE-Proton, and Wine inventory plus backend launch through a selected runner are implemented; a real-game smoke test remains.

- Discover Proton, GE-Proton, and Wine installations.
- Validate runner paths and versions.
- Launch one supported Windows game through the Phase 06 lifecycle.

Runner inventory is backend-only at this point. Linux scanning checks each canonical Steam root in the existing default-root order, then its custom `compatibilitytools.d` directory, followed by the `steamapps/common` directory for that root and its configured libraries. Repeated roots and runner paths are deduplicated. Proton candidates need a Proton-named directory and an executable `proton` entrypoint. Wine discovery checks `wine`, then `wine64`, in PATH order and accepts only a successful version response. Discovery is separate from game catalog scanning.

Verification: available runners are detected without false success and a basic Windows-game launch reaches the actual game process on Linux.

### 07B: Compatibility configuration

Status: persistent global defaults, per-game overrides, effective-value merging, validation, and launch integration are implemented in the Rust backend. Platform behavior and dedicated runtime, overlay, and graphics options remain open.

- Persist runner selection, global defaults, per-game overrides, prefix roots and paths, environment variables, DLL overrides, arguments, and working directories.
- Validate and apply the effective configuration immediately before launching a manually imported Windows game.
- Define and support Steam Runtime behavior, overlay integration, and dedicated graphics, WineD3D, and Wayland options where canonical requirements allow.
- Preserve process handling and actionable diagnostics.

Verification: each supported setting changes the observable launch environment, invalid combinations fail before launch, and diagnostics identify the applied configuration.

### 07C: OFLL parity matrix

- Record the exact upstream Online Fix Linux Launcher revision under review.
- Map each required behavior to Legio ownership and evidence.
- Cover debug logs, shortcuts, icons, and supported game or fix behavior.
- Mark unsupported or intentionally excluded behavior explicitly.

Verification: every required upstream responsibility has a Rust implementation and an exercised scenario, or an approved documented exclusion.

## Dependencies

Phase 06 supplies the shared launch lifecycle, sessions, process tracking, and diagnostics contract.

## Completion criteria

Runner discovery and configuration are reliable, supported Windows games launch on Linux, failures are diagnosable, and the required OFLL capability set has revision-specific evidence.

## Open technical decisions

- Record the exact upstream OFLL revision before claiming parity.
- Establish which Steam Runtime, overlay, graphics, WineD3D, and Wayland combinations are supportable with real games before claiming 07B complete.

## Update notes

Runner inventory is available through the `list_compatibility_runners` Tauri command on Linux. It reports validated Proton, GE-Proton, and Wine paths with version metadata.

PR #34 adds the Linux-only `launch_game_with_runner` command for manually imported Windows `.exe` games. It revalidates the selected runner immediately before launch, uses the shared launch lifecycle, and identifies the game process through a per-launch token plus executable-name match. Each game gets an app-data compatibility prefix; Proton receives the Steam client path and compatibility data path, while Wine receives `WINEPREFIX`. Runner selection is not yet persisted. Steam-managed games continue to use the Steam launch path.

Tests cover process identification, launch and stop state transitions, spawn and wrapper failures, and cancellation before the game process appears. GitHub CI passes on Linux and Windows. 07A remains open until a supported Windows game is smoke-tested with an installed Proton or Wine runner.

07B backend support now persists global defaults and nullable per-game overrides, merges them into effective runner, prefix, arguments, environment, DLL overrides, and working directory values, validates the result, and applies it before launch. A game-specific runner can inherit the global choice; if none is configured, launch uses the first discovered runner. Environment values can express Proton options such as WineD3D, but Legio has no dedicated graphics, Steam Runtime, overlay, or Wayland controls yet. These runtime integrations require Linux game verification; the UI controls remain outside this backend handoff.
