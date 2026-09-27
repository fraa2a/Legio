# Frontend TODO

This checklist tracks product UI work against the backend available on current `main`. PR #36 (`feat/ui-upgrade`) is the active frontend integration branch and can be ahead of the checklist in some areas while behind current backend contracts in others. After updating that branch from `main`, check items off only when the current implementation is wired to the documented commands and verified in the app.

Use [`backend-contracts.md`](backend-contracts.md) for exact Tauri arguments, types, statuses, and backend behavior. Check items off only when the behavior is implemented and verified in the app.

## P0: Make the shell navigate and load real state

- [ ] Connect sidebar selection to app-level route/view state. Render Home, Library, Store, Downloads, and Settings in the main container. The current active section state is private to `Sidebar` and does not render a page.
- [ ] Add typed feature services and shared stores for games, settings, download queue, source snapshot, network status, and launch states. Keep `invoke()` calls inside those services.
- [ ] Hydrate persisted state at startup: app info, settings, `list_games`, `list_downloads`, and cached Legio source. Give each load a loading, ready, empty, and retryable error state.
- [ ] Refresh library and queue state after mutations. Prevent stale async responses from overwriting newer search or view state.
- [ ] Add accessible page headings, keyboard navigation, focus handling for dialogs, visible focus styles, labels for icon-only buttons, and reduced-motion behavior.

## P0: Library and game management

- [ ] Build a Library view backed by `list_games`, with search, sorting, filters, empty state, loading state, and visible distinctions for Steam-managed and manually imported games.
- [ ] Add Steam redetection: `scan_steam_installations` previews detected games and diagnostics. A separate explicit `import_steam_installations` action rescans and updates the persistent library. Reload `list_games` after import and show inserted, updated, unchanged, removed, and diagnostic counts.
- [ ] Add manual game creation from a Steam App ID or selected executable. Support custom display name and editing/resetting automatic name overrides. Executable import currently creates an unassociated game with `steamAppId: null`; do not present it as automatically Steam-identified.
- [ ] Keep native file and folder selection for manual executable import behind the frontend dialog service. The backend accepts selected absolute paths and intentionally does not expose a general filesystem browser command.
- [ ] For executable selection, call `scan_game_executables({ directory, gameName })`, show every candidate with its score/signals, and require explicit selection when `selectedPath` is null. Then call `import_manual_game({ input: { executablePath, name } })`.
- [ ] Allow changing a manual game's selected executable with `set_game_executable`. Make the UI distinguish a new scan from changing the saved executable. Do not expose executable selection for Steam-managed games.
- [ ] Add game details/settings and delete confirmation. After create, update, delete, scan, or import, reconcile the library from the backend result or reload it.
- [ ] Add native image selection and reset controls for custom game icons and banners using `set_game_icon`, `set_game_banner`, `reset_game_icon`, and `reset_game_banner`. Load current overrides through the matching `get_*` commands and request `extract_game_icon` after import when an executable icon is available.
- [ ] Add Steam metadata display and refresh using `get_steam_details`. Render remote descriptions as text or sanitize HTML. Load images through `get_steam_asset`, convert returned bytes to a Blob URL, and revoke old URLs.

## P0: Store search and source trust

- [ ] Build Store search with a debounced query, local results first, refresh state, empty state, and retryable errors. Use `search_catalog` for cached/local results and `refresh_catalog` to query Hydra through Rust. Do not call Hydra directly from Svelte. Preserve the relevance order returned by Rust; do not re-sort search results alphabetically.
- [ ] Handle out-of-order query responses so a slow response for an old query cannot replace current results. Refresh errors must leave cached results visible.
- [ ] Load the Legio source separately with `get_legio_source` and `refresh_legio_source`. Merge source availability by `steamAppId`; show cache freshness and warning state. Treat a missing or unpublished source manifest as an expected empty/unavailable state.
- [ ] Show source status clearly: `verified`, `unverified`, `unavailable`, or `unknown`. Require explicit confirmation before queuing an unverified release.
- [ ] Add a game detail view with Steam metadata, artwork, release version, size, and download availability. Search results do not add games to the library by themselves.

## P0: Downloads and installation

- [ ] Build a Downloads screen and a queue entry point in the shell. Poll `list_downloads` while relevant views are active; there is no download progress event today.
- [ ] Show per-job status, progress, rate, ETA, release version, and error. Treat `status` as an open string and preserve unrecognized future values.
- [ ] Wire pause, resume, retry, and cancel only for valid states. Refresh the queue after every action and show backend rejection messages. Treat `waiting` as a network-wait state without request churn; after a successful `check_steam_connectivity`, reload the queue because the backend resumes waiting jobs.
- [ ] Enqueue with `queue_download({ steamAppId, acceptUnverified })`; never fetch manifest download URLs directly from the frontend.
- [ ] After a job reaches `downloaded`, call `stage_download`, show extraction/validation failures, then call `scan_staged_executables` to list its relative executable candidates.
- [ ] Require explicit executable selection when `selectedRelativePath` is null and pass the selected `relativePath` as `executableRelative`. The backend revalidates containment and file type during finalization.
- [ ] Call `finalize_download` with the selected relative executable. On success reload both queue and library. Present conflicts/recovery errors without hiding staged evidence.
- [ ] Add a bandwidth limit control using `set_download_bandwidth_limit`; zero means unlimited. Decide whether the control belongs in Settings or Downloads.

## P0: Launch lifecycle and account override

- [ ] Add per-game Play/Cancel/Stop controls backed by `list_game_launch_states`, `launch_steam_game`, `cancel_game_launch`, and `stop_game`.
- [ ] Poll lifecycle state while a game is launching or running. Show Play for idle, Cancel while launching, and Stop while running. A successful launch command means accepted/requested, not that the game is running.
- [ ] Keep cancellation pending in local UI state until the backend reports idle or an error. The backend status enum currently has `idle`, `launching`, and `running`, with no `cancelling` value.
- [ ] Surface per-game launch errors next to the affected game and provide a retry path. Do not add frontend helper-process timing or Steam startup delays; the shared backend lifecycle and explicit-overlay Steam readiness gate own that behavior.
- [ ] Add the per-game saved Steam account selector. Populate it with `list_saved_steam_accounts`; save through `set_game_steam_account_preference`. The account list exposes display names and Steam IDs, not private account login names.
- [ ] Before launch, call `inspect_steam_game_launch`. On `mismatch`, ask the user before closing and restarting Steam, then launch with `confirmAccountSwitch: true`. Treat `unknown` as uncertain, never as a verified account match. Allow canceling the dialog without launching.
- [ ] Add account health using `check_game_steam_account`, including missing saved account, mismatch, and uncertain states.

## P1: Settings and system feedback

- [ ] Build Settings for theme, network/connectivity status, and actionable diagnostics from `get_network_log_status`.
- [ ] Add Linux compatibility runner discovery and global defaults using `list_compatibility_runners`, `get_compatibility_defaults`, and `save_compatibility_defaults`. Include typed Steam Runtime, overlay, WineD3D, Wayland, and debug logging controls with the backend's validation rules.
- [ ] Add per-game compatibility overrides using `get_game_compatibility_overrides` and `save_game_compatibility_overrides`. Expose inheritance/reset semantics for runner, prefix, arguments, working directory, environment, DLL overrides, Steam Runtime, overlay, WineD3D, Wayland, and debug logging.
- [ ] Launch manually imported Windows games with `launch_configured_game_with_runner`; keep `launch_game_with_runner` as an explicit testing command. Explain unsupported-platform behavior returned by the backend. Surface compatibility diagnostics/log location from `get_compatibility_logs_directory` when a launch fails.
- [ ] On Windows, expose per-game native arguments and working directory through `get_native_launch_config` and `save_native_launch_config`, then launch a manual executable with `launch_native_game`.
- [ ] Show source/network staleness and cached-data warnings without blocking locally available library actions. Keep cached catalog results, Steam details, source data, and stale artwork visible when refresh fails.

## P1: Home, polish, and verification

- [ ] Build Home around real library, queue data, and `get_playtime_summaries`. Show real total playtime and active-session state where useful. Do not invent recent-session timelines or the monthly heatmap until the backend exposes session history/per-day aggregates.
- [ ] Add consistent loading, empty, offline, stale, error, confirmation, and success states across pages. Home, Library, local metadata, playtime, settings, and permitted installed-game actions must remain useful offline; remote actions should expose an explicit retry state without loops.
- [ ] Review the section 35 product mockup when available and reconcile it with the current shell before final visual acceptance.
- [ ] Add component/service tests for query races, hydration failures, queue action eligibility, manual candidate selection, confirmation dialogs, and stale cache display.
- [ ] Manually verify Steam redetection, manual import, source search, unverified confirmation, download recovery, finalization, account switching, launch cancel/stop, and settings persistence on supported platforms.
- [ ] Run `pnpm check`, `pnpm lint`, and `pnpm build`; verify the Tauri app with the actual backend commands rather than mock-only UI state.

## Backend gaps to track instead of guessing in the UI

- [ ] Add automatic Steam App ID identification for manually selected executables as required by PLAN sections 17 and 18. There is no current command that derives a Steam identity from an EXE/folder/PE metadata, so the frontend must not fake this behavior.
- [ ] Expose session history or per-day playtime aggregates before implementing the full recent-played timeline and monthly activity heatmap. `get_playtime_summaries` currently provides totals and active-session counts.
- [ ] Add progress events for downloads and process lifecycle only if polling proves inadequate; there are no such events in the current contract.
- [ ] Define frontend commands only after backend/product decisions exist for runner acquisition, prefix cleanup/tools, and trusted fix packaging.
- [ ] Add application/game update contracts in Phase 09 before building update-check UI.
