# Frontend TODO

This checklist tracks the frontend currently merged into `main` through PR #36. Checked items describe behavior present in the code. Runtime checks that still need a real Windows or Linux session remain open.

Use [`backend-contracts.md`](backend-contracts.md) for exact Tauri arguments, types, statuses, and backend behavior. Keep interactive platform verification as separate open items.

## P0: Make the shell navigate and load real state

- [x] Connect sidebar selection to shared view state. Render Home, Library, Store, and Downloads in the main container; open Settings in a dialog.
- [x] Add typed feature services and shared stores for games, settings, download queue, source snapshot, network status, and launch states. Keep `invoke()` calls inside those services.
- [x] Hydrate persisted state at startup: app info, settings, `list_games`, `list_downloads`, and cached Legio source. Give each load a loading, ready, empty, and retryable error state.
- [x] Refresh library and queue state after mutations. Prevent stale async responses from overwriting newer search or view state.
- [ ] Complete keyboard and accessibility review. The shell has page headings, dialog focus handling, labels for icon-only buttons, and reduced-motion handling, and the global Tab blocker has been removed. Complete the remaining cross-page review in the app.

## P0: Library and game management

- [x] Build a Library view backed by `list_games`, with name search, alphabetical order, empty/loading states, and game cards with playtime.
- [ ] Add Library filters and visible Steam-managed versus manual game distinctions.
- [x] Sync Steam installations on startup and at the configured interval through `import_steam_installations`, then reload `list_games` and show import diagnostics.
- [ ] Add an explicit `scan_steam_installations` preview and display inserted, updated, unchanged, and removed counts before applying changes.
- [x] Add manual game creation from a selected executable or a game chosen through Steam catalog search. Support custom names and resettable automatic names. Executable selection previews Steam identity and automatically selects a unique match; ambiguous matches need a user choice.
- [x] Keep native file and folder selection for manual executable import behind the frontend dialog service. The backend accepts selected absolute paths and intentionally does not expose a general filesystem browser command.
- [x] For directory scans, call `scan_game_executables({ directory, gameName })` and show candidates with their scores/signals in game settings. Require an executable selection when `selectedPath` is null, then call `import_manual_game({ input: { executablePath, name } })`.
- [x] Allow changing a manual game's selected executable with `set_game_executable`. Make the UI distinguish a new scan from changing the saved executable. Do not expose executable selection for Steam-managed games.
- [x] Add game details/settings and delete confirmation. After create, update, delete, scan, or import, reconcile the library from the backend result or reload it.
- [x] Add native image selection and reset controls for custom game icons and banners using `set_game_icon`, `set_game_banner`, `reset_game_icon`, and `reset_game_banner`. Load overrides through the matching `get_*` commands and offer `extract_game_icon` for games with an executable.
- [x] Add Steam metadata display and refresh using `get_steam_details`. Render remote descriptions as text or sanitize HTML. Load images through `get_steam_asset`, convert returned bytes to a Blob URL, and revoke old URLs.

## P0: Store search and source trust

- [x] Build Store search with a debounced query, local results first, refresh state, empty state, and retryable errors. Use `search_catalog` for cached/local results and `refresh_catalog` to query Hydra through Rust. Do not call Hydra directly from Svelte. Preserve the relevance order returned by Rust; do not re-sort search results alphabetically.
- [x] Handle out-of-order query responses so a slow response for an old query cannot replace current results. Refresh errors must leave cached results visible.
- [x] Load the Legio source separately with `get_legio_source` and `refresh_legio_source`. Merge source availability by `steamAppId`; show cache freshness and warning state. Treat a missing or unpublished source manifest as an expected empty/unavailable state.
- [x] Show source status clearly: `verified`, `unverified`, `unavailable`, or `unknown`. Require explicit confirmation before queuing an unverified release.
- [x] Add a game detail view with Steam metadata, artwork, release version, size, and download availability. Search results do not add games to the library by themselves.

## P0: Downloads and installation

- [x] Build a Downloads screen and a queue entry point in the shell. Poll `list_downloads` while relevant views are active; there is no download progress event today.
- [x] Show per-job status, progress, rate, ETA, release version, and error. Treat `status` as an open string and preserve unrecognized future values.
- [x] Wire pause, resume, retry, and cancel only for valid states. Refresh the queue after every action and show backend rejection messages.
- [ ] Handle `waiting` without repeated download requests and reload the queue after a successful `check_steam_connectivity`; the current connectivity store updates network state but does not reload downloads.
- [x] Let a finished or abandoned entry leave the queue. Use `remove_download` for `cancelled`, `failed`, and `installed`, and `remove_finished_downloads` for bulk cleanup of completed rows. Keep retryable failures visible unless the user removes them explicitly.
- [x] Enqueue with `queue_download({ steamAppId, acceptUnverified })`; never fetch manifest download URLs directly from the frontend.
- [x] After a job reaches `downloaded`, call `stage_download`, show extraction/validation failures, then call `scan_staged_executables` to list its relative executable candidates.
- [x] Require explicit executable selection when `selectedRelativePath` is null and pass the selected `relativePath` as `executableRelative`. The backend revalidates containment and file type during finalization.
- [x] Call `finalize_download` with the selected relative executable. On success reload both queue and library. Present conflicts/recovery errors without hiding staged evidence.
- [x] Add a bandwidth limit control using `set_download_bandwidth_limit`; zero means unlimited. It lives in Settings and reads the persisted value through `get_download_bandwidth_limit`.
- [x] Let the user open the app-owned install directory from Settings through `open_installed_folder`. The frontend does not pass an arbitrary path.

## P0: Launch lifecycle and account override

- [x] Add per-game Play/Cancel/Stop controls backed by `list_game_launch_states`, `launch_steam_game`, `cancel_game_launch`, and `stop_game`.
- [x] Poll lifecycle state while a game is launching or running. Show Play for idle, Cancel while launching, and Stop while running. A successful launch command means accepted/requested, not that the game is running.
- [ ] Keep cancellation pending in local UI state until the backend reports idle or an error. The UI currently clears its pending flag when `cancel_game_launch` returns, while the backend status enum has no `cancelling` value.
- [x] Surface per-game launch errors next to the affected game and provide a retry path. Do not add frontend helper-process timing or Steam startup delays; the shared backend lifecycle and explicit-overlay Steam readiness gate own that behavior.
- [x] Add the per-game saved Steam account selector. Populate it with `list_saved_steam_accounts`; save through `set_game_steam_account_preference`. The account list exposes display names and Steam IDs, not private account login names.
- [x] Before launch, call `inspect_steam_game_launch`. On `mismatch`, ask the user before closing and restarting Steam, then launch with `confirmAccountSwitch: true`. Allow canceling the dialog without launching. `unknown` does not trigger an account switch.
- [ ] Add account health using `check_game_steam_account`, including missing saved account, mismatch, and uncertain states.

## P1: Settings and system feedback

- [ ] Build Settings for theme, network/connectivity status, and actionable diagnostics from `get_network_log_status`. Implemented; verify persistence and diagnostic errors in the app.
- [ ] Add Linux compatibility runner discovery and global defaults using `list_compatibility_runners`, `get_compatibility_defaults`, and `save_compatibility_defaults`. Include WineD3D, Wayland, and debug logging controls with the backend's validation rules. Implemented; verify on Linux.
- [ ] Add per-game compatibility overrides using `get_game_compatibility_overrides` and `save_game_compatibility_overrides`. Expose Launch via Steam and inheritance/reset semantics for runner, prefix, arguments, working directory, environment, DLL overrides, WineD3D, Wayland, and debug logging. Runtime and overlay follow Launch via Steam automatically. Implemented; verify inheritance and clearing in the app.
- [ ] On Linux, launch manually imported Windows games with `launch_configured_game_with_runner`; keep `launch_game_with_runner` as an explicit testing command. Explain unsupported-platform behavior returned by the backend. Surface compatibility diagnostics/log location from `get_compatibility_logs_directory` when a launch fails. Implemented; verify on Linux.
- [ ] On Windows, expose per-game native arguments and working directory through `get_native_launch_config` and `save_native_launch_config`, then launch a manual executable with `launch_native_game`. Implemented; verify on Windows.
- [x] Show source/network staleness and cached-data warnings without blocking locally available library actions. Keep cached catalog results, Steam details, source data, and stale artwork visible when refresh fails.

Implementation update (2026-09-27): Settings now has theme, network diagnostics, download, and Linux compatibility sections. Linux defaults and per-game compatibility overrides are wired to the documented commands, including argument lists. Windows manual-game settings expose native arguments and working directory. These flows still need in-app verification before their checklist items are marked complete.

## P1: Home, polish, and verification

- [x] Build Home around real library and queue data.
- [x] Load `get_playtime_summaries` at startup and show total playtime in Library game cards and last-played dates in the game version selector.
- [x] Show real playtime and active-session state on Home, a last-played banner with launch/info/settings actions, other recent games, downloads, and a monthly activity calendar backed by `get_playtime_activity`. Calendar slots outside the month are blank; months can use six weeks. Browser interaction checks use fixture data; platform launch verification remains open.
- [ ] Add consistent loading, empty, offline, stale, error, confirmation, and success states across pages. Home, Library, local metadata, playtime, settings, and permitted installed-game actions must remain useful offline; remote actions should expose an explicit retry state without loops.
- [ ] Review the section 35 product mockup when available and reconcile it with the current shell before final visual acceptance.
- [ ] Add component/service tests for query races, hydration failures, queue action eligibility, manual candidate selection, confirmation dialogs, and stale cache display.
- [ ] Manually verify Steam redetection, manual import, source search, unverified confirmation, download recovery, finalization, account switching, launch cancel/stop, and settings persistence on supported platforms.
- [x] Run `pnpm check`, `pnpm lint`, and `pnpm build` in CI.
- [ ] Verify the Tauri app interactively with actual backend commands on Windows and Linux.

## Backend gaps to track instead of guessing in the UI

- [x] Native file and folder selection is resolved with `tauri-plugin-dialog` and the narrowly scoped `dialog:allow-open` permission.
- [x] Safe staged executable selection is resolved with `scan_staged_executables`, which returns download-bound relative paths consumed unchanged by the frontend.
- [ ] Verify automatic Steam App ID identification for manually selected executables as required by PLAN sections 17 and 18. Rust compares normalized executable and nearby folder titles against the cached and refreshed Hydra catalog. The add-game dialog previews a unique match and offers ambiguous candidates; verify the full import path in the app.
- [x] Expose per-day playtime aggregates through `get_playtime_activity` for the monthly Home calendar. Raw session history for a full session timeline is still unavailable.
- [ ] Add progress events for downloads and process lifecycle only if polling proves inadequate; there are no such events in the current contract.
- [ ] Define frontend commands only after backend/product decisions exist for runner acquisition, prefix cleanup/tools, and trusted fix packaging.
- [ ] Add application/game update contracts in Phase 09 before building update-check UI.
