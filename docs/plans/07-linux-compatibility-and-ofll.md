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

Status: the initial responsibility inventory is recorded for the upstream v2.7.1 commit below. Full parity remains open; unsupported behavior has not been approved for exclusion.

- Record the exact upstream Online Fix Linux Launcher revision under review.
- Map each required behavior to Legio ownership and evidence.
- Cover debug logs, shortcuts, icons, and supported game or fix behavior.
- Mark unsupported or intentionally excluded behavior explicitly.

#### Initial revision-specific inventory

Reviewed upstream: [Online Fix Linux Launcher v2.7.1 at commit `86528986f71c3da0972a670c08feb0bb70a3dbe2`](https://github.com/ZzEdovec/onlinefix-linux/tree/86528986f71c3da0972a670c08feb0bb70a3dbe2). This review used its [README](https://github.com/ZzEdovec/onlinefix-linux/blob/86528986f71c3da0972a670c08feb0bb70a3dbe2/README.md), [launch and runtime handling](https://github.com/ZzEdovec/onlinefix-linux/blob/86528986f71c3da0972a670c08feb0bb70a3dbe2/src/app/modules/FilesWorker.php), and [per-game controls](https://github.com/ZzEdovec/onlinefix-linux/blob/86528986f71c3da0972a670c08feb0bb70a3dbe2/src/app/forms/gameSettings.php). The upstream describes Proton/GE-Proton management, prefixes, Steam Runtime, overlay and graphics toggles, game fixes, debug logs, icons, desktop shortcuts, and optional game downloads. This is an inventory, not a parity claim.

| Responsibility in the reviewed upstream | Legio ownership and evidence | State |
| --- | --- | --- |
| Discover and choose Proton, GE-Proton, or Wine | `runner_discovery.rs` validates and lists local runners. Global and per-game runner values are persisted; configured launch uses the first discovered runner when no selection exists. Runner downloads and version management are absent. | Partial |
| Create and choose per-game prefixes | `game_lifecycle.rs` creates isolated per-game compatibility data and validates custom prefix roots or exact paths. Prefix cleanup and relocation controls are absent. | Backend implemented |
| Set environment variables, DLL overrides, launch arguments, and working directory | `database.rs` persists defaults and nullable per-game overrides. `game_lifecycle.rs` merges, validates, and applies them before launch. | Backend implemented |
| Select Steam Runtime, Steam overlay, graphics, WineD3D, or native Wayland options | Arbitrary environment values can express some Proton variables, but Legio has no typed settings or verified runtime integration for these options. | Missing |
| Detect Steam and launch through it | `steam_local.rs`, `steam_switch.rs`, and the shared `game_lifecycle.rs` path support Steam-managed games. This does not provide OFLL's OnlineFix/Epic launch flow. | Partial |
| Monitor and terminate game processes | `game_process.rs` and `game_lifecycle.rs` identify supported game processes, expose lifecycle state, stop games, and report stage failures. Manual Wine/Proton launches use a launch token plus executable matching. | Implemented for current launch paths |
| Apply OnlineFix, FreeTP, Photon, EOS, or other game-specific patches | Legio does not apply these patches or install their game-specific files. | Missing; product scope decision required |
| Produce per-game Wine/Proton diagnostics and export logs | Legio has application diagnostics and launch errors, but no per-game Wine/Proton log collection or export flow. | Partial |
| Extract or select game icons and create desktop or application-menu shortcuts | Steam assets are cached for Steam-linked games. Manual-game icon extraction, custom icon persistence, and shortcuts are absent. | Partial |
| Download OFLL-compatible games and fixes | Legio's queue downloads entries from its own SHA256-verified source manifest and installs supported archives. It does not download OnlineFix/FreeTP releases or use OFLL/Hydra fix feeds. | Missing; source and product scope decision required |

Rows marked Missing or Partial remain open. No exclusion is approved by this inventory. The supported rows need exercised scenarios, including a real Linux game launch.

Verification: every required upstream responsibility has a Rust implementation and an exercised scenario, or an approved documented exclusion.

## Dependencies

Phase 06 supplies the shared launch lifecycle, sessions, process tracking, and diagnostics contract.

## Completion criteria

Runner discovery and configuration are reliable, supported Windows games launch on Linux, failures are diagnosable, and the required OFLL capability set has revision-specific evidence.

## Open technical decisions

- Define product scope and supported sources for game-specific fixes and external game downloads; no behavior is excluded yet.
- Verify typed Steam Runtime, overlay, graphics, WineD3D, and Wayland behavior with supported Linux games.
- Run a real supported Windows game through Proton or Wine to close 07A.

## Update notes

Runner inventory is available through the `list_compatibility_runners` Tauri command on Linux. It reports validated Proton, GE-Proton, and Wine paths with version metadata.

PR #34 adds the Linux-only `launch_game_with_runner` command for manually imported Windows `.exe` games. It revalidates the selected runner immediately before launch, uses the shared launch lifecycle, and identifies the game process through a per-launch token plus executable-name match. Each game gets an app-data compatibility prefix; Proton receives the Steam client path and compatibility data path, while Wine receives `WINEPREFIX`. Runner selection is not yet persisted. Steam-managed games continue to use the Steam launch path.

Tests cover process identification, launch and stop state transitions, spawn and wrapper failures, and cancellation before the game process appears. GitHub CI passes on Linux and Windows. 07A remains open until a supported Windows game is smoke-tested with an installed Proton or Wine runner.

07B backend support now persists global defaults and nullable per-game overrides, merges them into effective runner, prefix, arguments, environment, DLL overrides, and working directory values, validates the result, and applies it before launch. A game-specific runner can inherit the global choice; if none is configured, launch uses the first discovered runner. Environment values can express Proton options such as WineD3D, but Legio has no dedicated graphics, Steam Runtime, overlay, or Wayland controls yet. These runtime integrations require Linux game verification; the UI controls remain outside this backend handoff.
