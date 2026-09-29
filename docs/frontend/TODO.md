# Frontend TODO

This checklist tracks product UI work against the backend available on current `main`. PR #36 (`feat/ui-upgrade`) is the active frontend integration branch and can be ahead of the checklist in some areas while behind current backend contracts in others. After updating that branch from `main`, check items off only when the current implementation is wired to the documented commands and verified in the app.

Use [`backend-contracts.md`](backend-contracts.md) for exact Tauri arguments, types, statuses, and backend behavior. Check items off only when the behavior is implemented and verified in the app.

## P0: Make the shell navigate and load real state

- [x] Connect sidebar selection to app-level route/view state. Render Home, Library, Store, Downloads, and Settings in the main container. The current active section state is private to `Sidebar` and does not render a page.
- [x] Add typed feature services and shared stores for games, settings, download queue, source snapshot, network status, and launch states. Keep `invoke()` calls inside those services.
- [x] Hydrate persisted state at startup: app info, settings, `list_games`, `list_downloads`, and cached Legio source. Give each load a loading, ready, empty, and retryable error state.
- [x] Refresh library and queue state after mutations. Prevent stale async responses from overwriting newer search or view state.
- [x] Add accessible page headings, keyboard navigation, focus handling for dialogs, visible focus styles, labels for icon-only buttons, and reduced-motion behavior.

## P0: Library and game management

- [x] Build a Library view backed by `list_games`, with search, sorting, filters, empty state, loading state, and visible distinctions for Steam-managed and manually imported games.
- [x] Add Steam redetection: `scan_steam_installations` previews detected games and diagnostics. A separate explicit `import_steam_installations` action rescans and updates the persistent library. Reload `list_games` after import and show inserted, updated, unchanged, removed, and diagnostic counts.
- [x] Add manual game creation from a Steam App ID or selected executable. Support custom display name and editing/resetting automatic name overrides. Executable import currently creates an unassociated game with `steamAppId: null`; do not present it as automatically Steam-identified.
- [x] Keep native file and folder selection for manual executable import behind the frontend dialog service. The backend accepts selected absolute paths and intentionally does not expose a general filesystem browser command.
- [x] For executable selection, call `scan_game_executables({ directory, gameName })`, show every candidate with its score/signals, and require explicit selection when `selectedPath` is null. Then call `import_manual_game({ input: { executablePath, name } })`.
- [x] Allow changing a manual game's selected executable with `set_game_executable`. Make the UI distinguish a new scan from changing the saved executable. Do not expose executable selection for Steam-managed games.
- [x] Add game details/settings and delete confirmation. After create, update, delete, scan, or import, reconcile the library from the backend result or reload it.
- [ ] Add native image selection and reset controls for custom game icons and banners using `set_game_icon`, `set_game_banner`, `reset_game_icon`, and `reset_game_banner`. Load current overrides through the matching `get_*` commands and request `extract_game_icon` after import when an executable icon is available.
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
- [x] Wire pause, resume, retry, and cancel only for valid states. Refresh the queue after every action and show backend rejection messages. Treat `waiting` as a network-wait state without request churn; after a successful `check_steam_connectivity`, reload the queue because the backend resumes waiting jobs.
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
- [x] Keep cancellation pending in local UI state until the backend reports idle or an error. The backend status enum currently has `idle`, `launching`, and `running`, with no `cancelling` value.
- [x] Surface per-game launch errors next to the affected game and provide a retry path. Do not add frontend helper-process timing or Steam startup delays; the shared backend lifecycle and explicit-overlay Steam readiness gate own that behavior.
- [x] Add the per-game saved Steam account selector. Populate it with `list_saved_steam_accounts`; save through `set_game_steam_account_preference`. The account list exposes display names and Steam IDs, not private account login names.
- [ ] Before launch, call `inspect_steam_game_launch`. On `mismatch`, ask the user before closing and restarting Steam, then launch with `confirmAccountSwitch: true`. Treat `unknown` as uncertain, never as a verified account match. Allow canceling the dialog without launching.
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
- [ ] Wire `get_playtime_summaries` into Home and Library for real total playtime and active-session state. Do not invent recent-session timelines or the monthly heatmap until the backend exposes session history/per-day aggregates.
- [ ] Add consistent loading, empty, offline, stale, error, confirmation, and success states across pages. Home, Library, local metadata, playtime, settings, and permitted installed-game actions must remain useful offline; remote actions should expose an explicit retry state without loops.
- [ ] Review the section 35 product mockup when available and reconcile it with the current shell before final visual acceptance.
- [ ] Add component/service tests for query races, hydration failures, queue action eligibility, manual candidate selection, confirmation dialogs, and stale cache display.
- [ ] Manually verify Steam redetection, manual import, source search, unverified confirmation, download recovery, finalization, account switching, launch cancel/stop, and settings persistence on supported platforms.
- [x] Run `pnpm check`, `pnpm lint`, and `pnpm build`; verify the Tauri app with the actual backend commands rather than mock-only UI state.

## Backend gaps to track instead of guessing in the UI

- [x] Native file and folder selection is resolved with `tauri-plugin-dialog` and the narrowly scoped `dialog:allow-open` permission.
- [x] Safe staged executable selection is resolved with `scan_staged_executables`, which returns download-bound relative paths consumed unchanged by the frontend.
- [ ] Verify automatic Steam App ID identification for manually selected executables as required by PLAN sections 17 and 18. Rust now compares exact normalized executable and nearby folder titles against the cached and refreshed Hydra catalog. A unique match is linked automatically; duplicate exact matches require a user choice. Keep this open until verified in the app.
- [ ] Expose session history or per-day playtime aggregates before implementing the full recent-played timeline and monthly activity heatmap. `get_playtime_summaries` currently provides totals and active-session counts.
- [ ] Add progress events for downloads and process lifecycle only if polling proves inadequate; there are no such events in the current contract.
- [ ] Define frontend commands only after backend/product decisions exist for runner acquisition, prefix cleanup/tools, and trusted fix packaging.
- [ ] Add application/game update contracts in Phase 09 before building update-check UI.
