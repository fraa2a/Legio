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

Status: the revision-specific behavior inventory and canonical conflict audit are complete for the upstream commit below. Implementation parity remains open.

- Record the exact upstream Online Fix Linux Launcher revision under review.
- Map each required behavior to Legio ownership and evidence.
- Cover debug logs, shortcuts, icons, and supported game or fix behavior.
- Mark unsupported or intentionally excluded behavior explicitly.

#### Revision-specific responsibility and parity matrix

Reviewed upstream: [Online Fix Linux Launcher v2.7.1 at commit `86528986f71c3da0972a670c08feb0bb70a3dbe2`](https://github.com/ZzEdovec/onlinefix-linux/tree/86528986f71c3da0972a670c08feb0bb70a3dbe2). The local `/home/fraa/Documents/OFLL` source was compared with a detached checkout of this exact commit; its `src/app` tree matched. The audit covered the tracked application modules and forms, including [FilesWorker](https://github.com/ZzEdovec/onlinefix-linux/blob/86528986f71c3da0972a670c08feb0bb70a3dbe2/src/app/modules/FilesWorker.php), [FixParser](https://github.com/ZzEdovec/onlinefix-linux/blob/86528986f71c3da0972a670c08feb0bb70a3dbe2/src/app/modules/FixParser.php), [game settings](https://github.com/ZzEdovec/onlinefix-linux/blob/86528986f71c3da0972a670c08feb0bb70a3dbe2/src/app/forms/gameSettings.php), [launcher settings](https://github.com/ZzEdovec/onlinefix-linux/blob/86528986f71c3da0972a670c08feb0bb70a3dbe2/src/app/forms/launcherSettings.php), [game import](https://github.com/ZzEdovec/onlinefix-linux/blob/86528986f71c3da0972a670c08feb0bb70a3dbe2/src/app/forms/newGameConfigurator.php), [game management](https://github.com/ZzEdovec/onlinefix-linux/blob/86528986f71c3da0972a670c08feb0bb70a3dbe2/src/app/forms/MainForm.php), [RAR handling](https://github.com/ZzEdovec/onlinefix-linux/blob/86528986f71c3da0972a670c08feb0bb70a3dbe2/src/app/modules/RarExtractor.php), [FreeTP installer](https://github.com/ZzEdovec/onlinefix-linux/blob/86528986f71c3da0972a670c08feb0bb70a3dbe2/src/app/modules/ftpInstaller.php), and [README claims](https://github.com/ZzEdovec/onlinefix-linux/blob/86528986f71c3da0972a670c08feb0bb70a3dbe2/README.md). Upstream claims below describe its documented behavior; they are not independent compatibility tests.

| Upstream responsibility | Legio implementation and evidence on `main` at `916ddb0` | State |
| --- | --- | --- |
| Discover Proton, GE-Proton, and Wine | `runner_discovery.rs` discovers local Proton-named tools, GE-Proton, and validated Wine executables. Discovery reports diagnostics. | Implemented for local installations |
| Install, update, remove, and select Proton versions | The compatibility schema persists global and per-game runner selections. Launch revalidates an installed path. There is no release catalogue, download, install, or removal service. | Partial; runner management remains |
| Create per-game prefixes and configure a prefix path | `game_lifecycle.rs` creates isolated per-game prefixes and validates custom roots or exact paths. It does not relocate, reset, or remove an existing prefix. | Backend implemented; management remains |
| Configure environment, DLL overrides, arguments before and after the executable, and working directory | Schema v14 persists global defaults and nullable per-game overrides. Effective configuration merges, validates, and applies structured process arguments and environment values. | Backend implemented |
| Select Steam Runtime | Typed global and per-game values select an installed local Steam Linux Runtime wrapper. Proton 11+ maps to Runtime 4, Proton 8-10 to sniper, and Proton 5.13-7 to soldier. Unsupported versions or missing runtimes fail with diagnostics; Legio does not download runtimes. | Backend implemented; real-game effect pending |
| Detect Steam and start it when a compatibility launch needs it | The Steam-managed launch and account-switch path can detect and start Steam. A manual Proton launch resolves the Steam client path but does not ensure that Steam is running or wait for sign-in. | Partial |
| Configure Steam overlay | Typed overlay configuration validates both renderer libraries, sets the overlay layer and default fake App ID 480 for manual games, and merges or removes `LD_PRELOAD` entries. The actual overlay has not been observed in a game. | Backend implemented; real-game effect pending |
| Configure graphics, WineD3D, and Wayland | Typed renderer and Wayland modes set Proton options and reject unsupported runner combinations. Wayland is currently limited to GE-Proton. | Backend implemented; real-game effect pending |
| Apply per-game settings and global defaults | Database inheritance and reset are implemented for typed compatibility settings and the other launch fields. Tauri backend commands expose persistence; frontend controls are Phase 08. | Backend implemented |
| Monitor and stop Wine/Proton game processes | `game_process.rs` and the shared `game_lifecycle.rs` track manual launches by launch token and executable, expose launch state, stop games, and report lifecycle-stage failures. | Backend implemented; real-game exercise pending |
| Provide debug mode, capture process output, and collect Wine/Proton diagnostics | Compatibility launch currently discards child stdout and stderr. Launch failures and applied typed settings are available in state, but there is no per-game debug mode, process log, log export, or Wine/Proton diagnostic bundle. User-provided `WINEDEBUG` can be passed as an environment value but does not provide collection. | Missing / partial |
| Fetch Steam game covers and allow game-specific banners | Steam-linked games use the Steam asset cache. Manual compatibility games do not have OFLL's Steam header fetch or a persisted custom banner flow. | Partial |
| Extract or choose a game icon | No executable icon extraction or persisted custom icon flow is implemented for manually imported games. | Missing |
| Create desktop and application-menu shortcuts | Legio does not generate per-game `.desktop` shortcuts. | Missing |
| Scan imported files for OnlineFix, FreeTP, EOSFix, SteamFix, and Photon metadata; derive DLL overrides or apply the Photon Newtonsoft workaround | Manual import selects Windows executables, but Legio does not identify or patch these fix layouts. Existing user-configured DLL overrides remain available. | Missing; fix behavior requires a product and trust decision |
| Install a FreeTP installer selected beside a game and automatically import the result | Legio does not launch sidecar installers or execute fix installers. | Missing; no canonical installer contract exists |
| Run prefix tools or an arbitrary selected executable inside a prefix | Legio has no backend operation for Wine utilities such as winecfg, regedit, explorer, taskmgr, or winetricks, and no run-in-prefix operation. | Missing; confirm required utility scope against the canonical plan |
| Remove a game together with optional files, prefixes, shortcuts, icons, and banners | Game records can be removed, but Legio does not implement OFLL's associated filesystem cleanup controls. | Partial; destructive cleanup needs an explicit safe backend contract |
| Download games or fixes from aria2 and an OnlineFix/Hydra source | Legio downloads only releases listed by its single JSON source and verifies declared SHA256 hashes. It does not query Hydra or OnlineFix feeds. | OFLL feed behavior conflicts with `PLAN.md` sections 8-10; do not add a second feed or unverified external source |
| Read or extract encrypted or multipart RAR archives, including the default `online-fix.me` password | Legio's archive policy in `PLAN.md` section 15 permits only single-part, unencrypted archives. OFLL's password and multipart behavior is deliberately excluded. | Incompatible with canonical archive policy |

#### Upstream fix compatibility claims

The v2.7.1 README makes these claims, and its `FixParser`, game import, and launch code implement fix-specific detection or modifications. Legio has not reproduced or independently verified these claims:

| Fix combination claimed by OFLL | OFLL v2.7.1 claim | Legio state |
| --- | --- | --- |
| OnlineFix SteamFix, 64-bit | Full support | Not implemented |
| OnlineFix SteamFix, 32-bit | May have issues | Not implemented |
| FreeTP SteamFix | Supports only fixes released before 2026 | Not implemented |
| Photon Launcher custom OnlineFix servers | Full support, including a Newtonsoft.Json workaround | Not implemented |
| OnlineFix SteamFix combined with EOSFix | Full support | Not implemented |
| FreeTP combined with EOSFix | Completely broken | Not implemented |
| EOSFix with OnlineFix through EOSAuthHooker | Supported with EOSAuthHooker; legacy mode is untested | Not implemented |
| FreeTP with EOSFix | Completely broken | Not implemented |

These are upstream claims, not Legio compatibility guarantees. Applying bundled or downloaded DLLs, modifying game files, clearing fix protection flags, or running an external fix installer needs an explicit product decision and a trusted, auditable installation contract. Legio must not add arbitrary scripts, hooks, or unverified fix downloads to approximate this behavior.

#### Canonical conflicts and scope

| OFLL behavior | Canonical constraint or disposition |
| --- | --- |
| Direct downloads from an OnlineFix/Hydra source through aria2 | `PLAN.md` sections 8-10 define one Legio-controlled JSON source, a fixed schema, trust lists, and SHA256-verified releases. Do not add another provider or bypass this trust model. The existing Legio release model may be considered for approved fix packages after a product decision. |
| Password-protected or multipart RAR support | `PLAN.md` section 15 explicitly limits initial support to single-part, unencrypted archives. Do not implement OFLL's password prompt, default password, or multipart extraction. |
| Running FreeTP sidecar installers or applying Photon, EOS, SteamFix, or OnlineFix file patches | `PLAN.md` forbids source-defined scripts, hooks, and arbitrary execution commands. The required safe behavior and permitted fix sources are not yet specified. Keep these features blocked pending a product decision; a Legio-controlled static release may be evaluated separately. |
| OFLL's `fake Steam` mode replacing SteamFix DLLs | The behavior mutates game files and changes multiplayer fix behavior. No equivalent Legio contract exists. It remains unimplemented pending a product decision and trusted package/rollback design. |
| Runner and Steam Runtime downloads | Local runner discovery and selection are implemented. Download provenance, integrity metadata, installation, update, and removal behavior must be designed within Legio's canonical trust rules before implementation. |
| OFLL settings UI, localization, donation links, updater, and fullscreen launcher | These are not Linux compatibility backend responsibilities in Phase 07. Frontend work belongs to Phase 08, and the other application features are outside this parity matrix unless the canonical plan assigns them later. |

No OFLL behavior listed as missing has been silently approved for permanent exclusion. The explicit archive and source conflicts above follow `PLAN.md`; unresolved fix packaging and execution behavior remain product decisions. 07C is complete only when each canonical requirement has an implementation and exercised evidence, or a canonical-compatible disposition is recorded.

Verification: the audit is tied to an exact upstream commit and covers the relevant tracked application modules and forms, the README compatibility claims, and Legio's canonical source/archive trust rules. Feature implementation and exercised scenarios remain open as shown in the matrix.

## Dependencies

Phase 06 supplies the shared launch lifecycle, sessions, process tracking, and diagnostics contract.

## Completion criteria

Runner discovery and configuration are reliable, supported Windows games launch on Linux, failures are diagnosable, and the required OFLL capability set has revision-specific evidence.

## Open technical decisions

- Define the safe, canonical-compatible scope and source model for OnlineFix, FreeTP, EOSFix, Photon, SteamFix, and other game-specific behavior. The upstream direct Hydra feed, sidecar installer, and file mutation flows are not an approved design.
- Define runner download, installation, update, and removal behavior, including integrity metadata and installation locations.
- Define which prefix utilities, debug logs, shortcuts, icons, and filesystem cleanup operations are required in the backend contract. Any file removal must remain scoped to confirmed Legio-owned paths.
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
