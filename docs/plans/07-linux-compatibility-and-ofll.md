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

Status: Proton, GE-Proton, and Wine inventory plus backend launch through a selected runner are implemented. Terraria and Proton 10 are installed; a real-game smoke test is pending.

- Discover Proton, GE-Proton, and Wine installations.
- Validate runner paths and versions.
- Launch one supported Windows game through the Phase 06 lifecycle.

Runner inventory is backend-only at this point. Linux scanning checks each canonical Steam root in the existing default-root order, then its custom `compatibilitytools.d` directory, followed by the `steamapps/common` directory for that root and its configured libraries. Repeated roots and runner paths are deduplicated. Proton candidates need a Proton-named directory and an executable `proton` entrypoint. Wine discovery checks `wine`, then `wine64`, in PATH order and accepts only a successful version response. Discovery is separate from game catalog scanning.

Verification: available runners are detected without false success and a basic Windows-game launch reaches the actual game process on Linux.

### 07B: Compatibility configuration

Status: persistent global defaults, per-game overrides, effective-value merging, validation, typed runtime, overlay, graphics, WineD3D, and Wayland options are implemented in the Rust backend. Their in-game effects still need verification with a supported Windows game.

- Persist runner selection, global defaults, per-game overrides, prefix roots and paths, environment variables, DLL overrides, arguments, and working directories.
- Validate and apply the effective configuration immediately before launching a manually imported Windows game.
- Support Steam Runtime selection, overlay integration, WineD3D, and Wayland through typed settings with validation and diagnostics.
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
| Select Steam Runtime, Steam overlay, graphics, WineD3D, or native Wayland options | Schema v14 persists typed global defaults and nullable per-game overrides. The effective values select a local Steam Linux Runtime wrapper for Proton 11+ (runtime 4), Proton 8-10 (sniper), or Proton 5.13-7 (soldier); older Proton versions and unsupported architectures return an error. No runtime download is attempted. Overlay enablement checks for both Steam renderer libraries, sets the overlay layer and OFLL's default `SteamOverlayGameId` 480 for manual games, and updates `LD_PRELOAD`; disablement removes overlay injection while retaining unrelated preload entries. WineD3D and Wayland set Proton environment options. Wayland is restricted to GE-Proton. Launch state exposes the selected runner and typed options. | Backend implemented; real-game effect verification pending |
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
- Verify typed Steam Runtime, overlay, WineD3D, and Wayland effects with a supported Linux game. Current automated tests verify the wrapper arguments, child environment, validation failures, persistence, inheritance, and diagnostics, not in-game rendering or overlay behavior.
- Find an installed Windows game that reaches its game process, then exercise it through Legio's launch, process tracking, stop, and session lifecycle to close 07A.

## Update notes

Runner inventory is available through the `list_compatibility_runners` Tauri command on Linux. It reports validated Proton, GE-Proton, and Wine paths with version metadata.

PR #34 added the Linux-only `launch_game_with_runner` command for manually imported Windows `.exe` games. It revalidates the selected runner immediately before launch, uses the shared launch lifecycle, and identifies the game process through a per-launch token plus executable-name match. Each game gets an app-data compatibility prefix; Proton receives the Steam client path and compatibility data path, while Wine receives `WINEPREFIX`. At that point, runner selection was not persisted. Steam-managed games continue to use the Steam launch path.

Tests cover process identification, launch and stop state transitions, spawn and wrapper failures, and cancellation before the game process appears. GitHub CI passes on Linux and Windows. 07A remains open until a supported Windows game is smoke-tested with an installed Proton or Wine runner.

#### Real-game smoke evidence, 2026-09-26

Platform: Linux x86_64 under Wayland. The detected local installation contains Proton 10.0, Steam Linux Runtime sniper, and Terraria App ID 105600. The game executable is a Windows PE file. The smoke used a new prefix under `/tmp`, not the installed game's Steam prefix:

```sh
env STEAM_COMPAT_CLIENT_INSTALL_PATH=/home/fraa/.local/share/Steam \
  STEAM_COMPAT_DATA_PATH=/tmp/legio-phase07-terraria.ZPifa7 \
  /mnt/HDD/Games/Steam/steamapps/common/SteamLinuxRuntime_sniper/run -- \
  '/mnt/HDD/Games/Steam/steamapps/common/Proton 10.0/proton' run \
  /mnt/HDD/Games/Steam/steamapps/common/Terraria/Terraria.exe
```

Observed: the runtime started Proton 10.1000-105, created the temporary prefix, and Wine reported `fsync: up and running`. Terraria then exited with `System.DllNotFoundException: SDL3.dll`; no game process was observed. This is a failed game launch, not a completed smoke test. A second attempt with ROUNDS did not reach its game process: pressure-vessel remained blocked reading from the `/mnt/HDD` filesystem and the attempt was terminated. 07A remains in progress.

07B now persists `steamRuntime`, `steamOverlay`, `graphicsRenderer`, and `wayland` as enums in schema v14, both as global defaults and nullable game overrides. The effective configuration applies local runtime wrapping, Steam overlay injection, WineD3D, and GE-Proton Wayland options. Unknown enum values are rejected at deserialization. Environment entries cannot override variables owned by typed settings, and unsupported runner combinations fail before spawning. Missing runtime or overlay files produce actionable errors. The `list_game_launch_states` response reports the runner and typed configuration used for a compatibility launch. UI controls remain out of scope for this backend change.

The Steam Runtime mapping follows OFLL v2.7.1's Proton version mapping, but Legio requires the mapped runtime to already exist in a detected Steam library. Overlay uses OFLL's default fake App ID 480 for Legio manual games, which have no Steam App ID. Legio validates Steam's 32-bit and 64-bit renderer files and refuses to combine its overlay injection with a configured custom `LD_PRELOAD`. WineD3D and Wayland values are applied through the documented Proton environment options; native Wayland currently requires GE-Proton. These are backend integration results, not a claim that the runtime, overlay, or rendering behavior has been observed in-game.

Automated tests now cover schema migration and persistence, inheritance and reset, environment application, wrapper argument structure, invalid runner or environment combinations, missing overlay/runtime diagnostics, and launch-state diagnostics. 07A remains in progress until a real supported game process is observed through Proton and exercised through Legio's own launch and stop lifecycle.
