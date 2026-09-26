# Frontend developer guide and backend contracts

This is the implementation guide for wiring the Legio UI to the Rust/Tauri backend. Keep it aligned with the registered commands in [`src-tauri/src/lib.rs`](../../src-tauri/src/lib.rs), their request and response types, and the frontend service modules under [`src/lib/services`](../../src/lib/services).

The current UI redesign is PR #36 (`feat/ui-upgrade`). It adds the Svelte 5/Tailwind shell, window controls, and reusable primitives, but the main content is empty and the sidebar state does not select a page. Feature, store, type, and utility directories are placeholders. Implement the product flows in `src/lib/features/{home,library,store,downloads,settings}`, with shared state in `src/lib/stores`; keep all Tauri calls inside typed service modules. See [`TODO.md`](TODO.md) for the feature-by-feature completion checklist.

The UI should call Tauri only from a feature service module. Components should consume typed service functions and own presentation state, not duplicate backend rules. Rust is the source of truth for library records, installation state, queue state, compatibility settings, and process state.

## Contract conventions

- Rust structs marked `rename_all = "camelCase"` serialize as camelCase JSON. Enum variants marked `snake_case` use values such as `verified`, `already_matches`, and `running`.
- Tauri command arguments use camelCase names derived from Rust parameters: `gameId`, `steamAppId`, `acceptUnverified`, `bytesPerSecond`.
- Commands returning `Result<T, String>` reject the `invoke` promise with a string. Catalog and Steam detail commands reject with a structured object containing `kind`, `message`, and optional `status`.
- There are currently no queue or game-lifecycle push events. Refresh state by polling the corresponding list command while the view is active. Avoid one poll loop per row.
- Do not persist backend-owned state in browser storage. Rehydrate it from commands on app startup and after mutations.

## App startup and preferences

Call these through `invoke` wrappers:

| Command | Arguments | Result |
| --- | --- | --- |
| `get_app_info` | none | `{ name, version, platform }` |
| `get_settings` | none | `{ theme: "system" | "dark" | "light" }` |
| `save_settings` | `{ settings: { theme } }` | saved settings |
| `get_network_status` | none | `"unknown" | "online"` |
| `check_steam_connectivity` | none | `{ status, detail }` |
| `get_network_log_status` | none | `{ directory, lastError, droppedRecords, pendingRecords }` |

Suggested startup hydration: load app info, settings, `list_games`, `list_downloads`, and cached `get_legio_source`; show cached content immediately, then refresh network connectivity and source metadata in the background.

## Library and game settings

### Steam library

| Command | Arguments | Result |
| --- | --- | --- |
| `list_games` | none | `Game[]` |
| `scan_steam_installations` | none | `{ games: DetectedGame[], diagnostics: string[] }` preview only |
| `import_steam_installations` | none | `{ detected, inserted, updated, unchanged, removed, diagnostics }` |
| `create_game` | `{ input: { name, steamAppId } }` | created `Game` |
| `update_game` | `{ input: { id, steamAppId, automaticName, nameOverride } }` | updated `Game` |
| `remove_game` | `{ id }` | `void` |

`Game` currently serializes as:

```ts
interface Game {
  id: string;
  steamAppId: number | null;
  automaticName: string | null;
  nameOverride: string | null;
  name: string; // resolved display name
  steamInstallPath: string | null;
  steamAccountId: string | null;
  executablePath: string | null;
}
```

The frontend's `Game` type must include `executablePath`; it is easy to miss because it was added to the backend after the initial service type. Import preview is read-only. Import is a separate action that updates the persistent library. After import, call `list_games` again. The backend preserves manual rows and user naming when it refreshes Steam-managed rows.

For user-visible Steam redetection, call `scan_steam_installations` to show detected games and diagnostics, then call `import_steam_installations` only after the user chooses to import/update. The import command performs its own scan and reconciliation; the preview is not a transaction token and can differ if Steam changes between calls. Reload `list_games` after import. The serialized scan response does not expose excluded non-games such as compatibility tools.

Naming rules are enforced by the backend: `create_game` rejects an empty name and stores it as `nameOverride` with `automaticName` left null, so a manually created row has no automatic name to restore. `update_game` requires a non-null `automaticName` whenever `nameOverride` is null, so offer "restore automatic name" only when `automaticName` is not null. Show whether a row uses a custom name, and resolve the display name from the `name` field the backend returns.

### Manual executable import

| Command | Arguments | Result |
| --- | --- | --- |
| `scan_game_executables` | `{ directory, gameName? }` | `{ candidates: [{ path, score, signals }], selectedPath }` |
| `import_manual_game` | `{ input: { executablePath, name? } }` | created `Game` |
| `set_game_executable` | `{ gameId, executablePath }` | updated `Game` |

Present all candidates when `selectedPath` is null. `selectedPath` is a suggestion, not a substitute for a user's explicit choice when the scan is ambiguous. A successful import adds a manual game with a null `steamAppId` and a populated `executablePath`. `set_game_executable` rejects Steam-managed rows, so never offer this flow when `steamAppId` is not null. The optional `name` of `ManualImportInput` and the optional `gameName` of `scan_game_executables` must be sent as `null` when absent; the backend then derives the display name from the executable path.

Candidate `signals` are only `game_name_match` and `game_root`. Display them as hints, never as a decision, and keep the import action disabled until the user picks a path. `set_game_executable` only changes the stored path, so the UI must present a rescan as a separate, explicitly confirmed step.

Native selection uses `tauri-plugin-dialog` (`plugin:dialog|open`), registered in `src-tauri/src/lib.rs` with only the `dialog:allow-open` permission in `src-tauri/capabilities/default.json`. Wrap it in `src/lib/services/dialog.ts`; components must not import the plugin directly. `open({ directory: true })` returns the folder to scan, while `directory: false` returns an executable chosen directly. Both resolve to `null` when cancelled. A directly picked executable counts as an explicit selection and does not require a scan.

### Per-game Steam account override

| Command | Arguments | Result |
| --- | --- | --- |
| `list_saved_steam_accounts` | none | `{ accounts: [{ steamId, displayName }], diagnostics }` |
| `set_game_steam_account_preference` | `{ gameId, steamId: string | null }` | updated `Game`; null clears the override |
| `check_game_steam_account` | `{ gameId }` | `{ status, accountRequirementMet, selectedSteamId, message }` |

Do not expect `accountName` in the account list. It is intentionally private to the backend. Account check status is `not_required`, `missing_saved_account`, `match`, `mismatch`, or `unknown`. Treat `unknown` as inconclusive and display `message` when present.

Before launching an overridden Steam game, call `inspect_steam_game_launch({ gameId })`. It returns `{ status, steamRunning, currentAccountName, targetAccountName }`, with status `no_override`, `already_matches`, `mismatch`, or `unknown`. If status is `mismatch`, ask whether Steam may be closed and restarted, then call `launch_steam_game({ gameId, confirmAccountSwitch: true })` only after confirmation. For other statuses call with `false`. An `unknown` account must not be presented as a verified match.

### Compatibility settings for Linux manual games

These commands are available on `main` after PR #35.

| Command | Arguments | Result |
| --- | --- | --- |
| `get_compatibility_defaults` | none | global defaults |
| `save_compatibility_defaults` | `{ defaults }` | saved defaults |
| `get_game_compatibility_overrides` | `{ gameId }` | nullable per-game values |
| `save_game_compatibility_overrides` | `{ gameId, overrides }` | saved overrides |
| `list_compatibility_runners` | none | `{ runners, diagnostics }` |
| `launch_configured_game_with_runner` | `{ gameId }` | starts configured manual game |
| `get_playtime_summaries` | none | per-game session totals in milliseconds |

`get_playtime_summaries` returns `{ gameId, totalMilliseconds, activeSessions }` for each local game. Active totals are calculated through the request time. The backend starts a session after detecting the game process, closes it after the lifecycle monitor observes exit, and recovers sessions left open by a crash at their last heartbeat. Different games may have overlapping session time.

Defaults have `runnerPath`, `prefixRoot`, `argumentsBefore`, `argumentsAfter`, `workingDirectory`, `environment`, `dllOverrides`, `steamRuntime`, `steamOverlay`, `graphicsRenderer`, and `wayland`. Per-game overrides have those settings plus `prefixPath`; each typed option is nullable and inherits when null. Enum values are `steamRuntime: "runner_default" | "steam_linux_runtime"`, `steamOverlay: "runner_default" | "enabled" | "disabled"`, `graphicsRenderer: "runner_default" | "wine_d3d"`, and `wayland: "runner_default" | "disabled" | "native"`. For scalar/list values, `null` inherits; an empty string/list clears an inherited value. Environment and DLL maps merge by key; an empty map clears all inherited entries. Saving settings persists them, but the UI should not imply that settings were applied until the save command succeeds.

The backend wraps Proton in a locally installed Steam Linux Runtime only when `steamRuntime` is `steam_linux_runtime`; it does not download runtimes. WineD3D and Wayland set Proton environment options. Native Wayland is accepted only with GE-Proton. Steam overlay enablement requires the two Steam overlay renderer libraries in the detected Steam installation; the backend validates them before launch. Do not expose arbitrary environment overrides for variables managed by these typed settings.

`launch_game_with_runner({ gameId, runnerPath })` is an explicit-runner testing command. Production UI should use `launch_configured_game_with_runner` after selecting settings. Runner discovery and configured launch are Linux-only; on other platforms discovery returns no runners and a diagnostic.

### Native Windows manual games

| Command | Arguments | Result |
| --- | --- | --- |
| `get_native_launch_config` | `{ gameId }` | `{ arguments: string[], workingDirectory: string | null }` |
| `save_native_launch_config` | `{ gameId, config }` | saved config |
| `launch_native_game` | `{ gameId }` | `void`, accepted launch request |

Use these commands for manually imported Windows executables on Windows. Arguments are an array of exact process arguments, not a shell command. An empty or null working directory uses the executable's directory. The backend checks the executable and working directory again at launch. It tracks the launched process and descendants started within the executable's directory, then uses the shared Play, Cancel, Stop and session lifecycle. `launch_native_game` returns an unsupported-platform error elsewhere.

## Search and store metadata

### Search

| Command | Arguments | Result |
| --- | --- | --- |
| `search_catalog` | `{ query }` | cached/local `CatalogSearch` |
| `refresh_catalog` | `{ query }` | fetch, cache, and return `CatalogSearch` |

```ts
interface CatalogSearch {
  games: { steamAppId: number; name: string; availability: "unknown" | "unavailable" | "verified" | "unverified" }[];
  total: number;
  cachedAt: number | null;
  stale: boolean;
  sourceCachedAt: number | null;
  sourceStale: boolean;
}
```

Search the cache first, debounce user input, and use `refresh_catalog` for network refresh. A failed refresh rejects; keep the cached results visible and show the error. Catalog errors have `kind` values `invalid_query`, `invalid_response`, `timeout`, `network`, `http`, `too_large`, `database`, or `internal`. Catalog results are not automatically added to the library. To add an entry, either create a Steam-linked game or download a Legio source entry and finalize its install.

`refresh_catalog` is the only frontend entry point for online catalogue search. Rust sends `POST https://hydra-api-us-east-1.losbroxas.org/catalogue/search` with `{ title: query, take: 50, skip: 0 }`, accepts only Steam shop results, validates the response, and caches it. There is no pagination command. Queries are trimmed, must be nonempty, are limited to 200 UTF-8 bytes, and cannot contain control characters. The backend caps local results at 100 and marks cached catalog data stale after 24 hours. A refresh failure is an error, not a fallback response, so call `search_catalog` first and keep those results while refresh is in flight or failed.

Suggested page flow:

```ts
const local = await searchCatalog(query);
results = local.games;
catalogState = local.stale ? "stale" : "ready";

try {
  const refreshed = await refreshCatalog(query);
  if (requestId === latestRequestId) results = refreshed.games;
} catch (error) {
  if (requestId === latestRequestId) refreshError = messageFor(error);
}
```

Debounce the input and keep a monotonically increasing request ID to ignore stale responses. Update the existing catalog service type to include `availability`, `sourceCachedAt`, and `sourceStale` before using those fields in the UI.

Availability is joined by Steam App ID from the Legio manifest. `verified` and `unverified` indicate source trust, `unavailable` means no downloadable Legio release exists, and `unknown` means source status could not be established.

### Legio source

| Command | Arguments | Result |
| --- | --- | --- |
| `get_legio_source` | none | `{ manifest, cachedAt, stale, warning }` |
| `refresh_legio_source` | none | same shape |

Manifest v1 fields are `schemaVersion`, `generatedAt`, `verified[]`, and `unverified[]`. Each entry contains `steamAppId`, `name`, `release: { version, publishedAt }`, and `download: { url, sha256, sizeBytes }`. Use source entries only through `queue_download`; do not download directly from a URL in the frontend. If refresh fails but a valid cache exists, the backend returns the stale cache and a warning.

The source manifest URL is `https://source.taxphobia.top/store.json`. Hydra and this source are separate: Hydra provides searchable Steam identities and names; the Legio source describes releases and trust status. Join by `steamAppId`, keep the Hydra title as the catalog identity, and display the source release/version separately. The manifest may be unavailable or not yet published, so empty and unavailable states are normal.

### Steam metadata and image assets

| Command | Arguments | Result |
| --- | --- | --- |
| `get_steam_details` | `{ steamAppId, refresh }` | `{ details, cachedAt, stale }` |
| `get_steam_asset` | `{ steamAppId, asset, index? }` | `{ bytes, contentType, stale, cacheWarning }` |

Details include name, type, description, developers, publishers, genres, platform flags, release date, and Steam image URLs. Render remote descriptions as text or sanitize HTML before display. Asset kinds are `header`, `capsule`, and `screenshot`; screenshot requires its zero-based `index`. The asset command returns a number array, not a URL or base64 string. Convert it to a `Blob` using `contentType`, create an object URL, and revoke the URL when replaced or unmounted. Respect `stale` and `cacheWarning` while preserving a usable cached image.

## Downloads and installation

### Queue contract

| Command | Arguments | Result |
| --- | --- | --- |
| `list_downloads` | none | `DownloadJob[]` |
| `queue_download` | `{ steamAppId, acceptUnverified }` | new `DownloadJob` |
| `pause_download` | `{ id }` | `void` |
| `resume_download` | `{ id }` | `void` |
| `retry_download` | `{ id }` | `void` |
| `cancel_download` | `{ id }` | `void` |
| `set_download_bandwidth_limit` | `{ bytesPerSecond }` | `void`; zero means unlimited. The value persists across application restarts. |
| `stage_download` | `{ id }` | staged directory path |
| `scan_staged_executables` | `{ id, gameName? }` | `{ candidates: [{ relativePath, score, signals }], selectedRelativePath }` |
| `finalize_download` | `{ id, executableRelative }` | `void`; creates installed game and library row |

```ts
interface DownloadJob {
  id: string;
  steamAppId: number;
  name: string;
  releaseVersion: string;
  sizeBytes: number;
  downloadedBytes: number;
  speedBps: number;
  etaSeconds: number | null;
  status: string;
  error: string | null;
}
```

Current statuses are `queued`, `downloading`, `waiting`, `paused`, `failed`, `downloaded`, `staging`, `staged`, `finalizing`, `installed`, and `cancelled`. Treat status as an open string for forward compatibility. Typical actions: pause only `queued`/`downloading`/`waiting`; resume `paused`/`waiting`; retry `failed`; cancel active or queued states. The backend enforces valid transitions and reports invalid actions as rejected invokes.

`queue_download` requires `acceptUnverified: true` before an unverified source can be queued. Poll `list_downloads` while the queue screen is visible because there is no progress event yet. Refresh the library after a successful `finalize_download`. For install selection, stage the verified archive, call `scan_staged_executables({ id, gameName? })`, let the user select a candidate by `relativePath`, then pass it unchanged as `executableRelative`. Candidate paths use forward slashes on every platform. The command only scans a download whose stored status is `staged` and whose stage path matches the app-owned path. Finalization revalidates the path, containment, and executable file before installing. `stage_download` still returns a path for diagnostics; the UI does not need to inspect it or convert absolute paths.

For unverified releases, show the trust warning before queueing and send `acceptUnverified: true` only after explicit confirmation. Never infer trust from a Steam catalog result. The install sequence is enqueue, poll, stage, scan staged candidates, select an executable, finalize, and reload queue plus library. Treat unknown job status values as unrecognized instead of failing the page.

## Launch lifecycle

| Command | Arguments | Result |
| --- | --- | --- |
| `launch_steam_game` | `{ gameId, confirmAccountSwitch }` | `{ gameId, steamAppId }` accepted launch request |
| `launch_game_with_runner` | `{ gameId, runnerPath }` | `void`, testing path |
| `launch_configured_game_with_runner` | `{ gameId }` | `void`, configured manual game launch |
| `launch_native_game` | `{ gameId }` | `void`, Windows manual game launch |
| `list_game_launch_states` | none | `GameLaunchState[]` |
| `cancel_game_launch` | `{ gameId }` | `void` |
| `stop_game` | `{ gameId }` | `void` |

Launch state status is currently `idle`, `launching`, or `running`. The launch command returning successfully means launch was requested, not that the game process is running. Poll state while a game is launching or running. Show `cancel` while `launching`, call `cancel_game_launch`, then reconcile to idle/error from the state list. Show `stop` only when running and call `stop_game`. Surface the state's `error` and rejected command errors. There is no `cancelling` backend state in this contract, so if the UI displays one, treat it as transient local presentation until the next backend state confirms the outcome.

For configured Linux compatibility launches, `compatibilityOptions` reports the selected runner name and version plus the typed options accepted for that launch. It is omitted for launch paths without compatibility options.

```ts
interface GameLaunchState {
  gameId: string;
  status: "idle" | "launching" | "running";
  error?: string;
  compatibilityOptions?: {
    runner: string;
    version: string;
    steamRuntime: "runner_default" | "steam_linux_runtime";
    steamOverlay: "runner_default" | "enabled" | "disabled";
    graphicsRenderer: "runner_default" | "wine_d3d";
    wayland: "runner_default" | "disabled" | "native";
  };
}
```

## Frontend organization and change checklist

- Keep one typed service per domain under `src/lib/services` (library, catalog, source, downloads, launch, game settings). Components should not import `invoke` directly.
- Keep shared API interfaces synchronized with Rust serialization. Check camelCase field names and snake_case enum values whenever Rust structs change.
- After every mutation, update or invalidate the owning store from the returned value where available, otherwise reload its list command.
- Keep cached content visible during refresh; show stale/warning/error states separately.
- Use backend validation as final authority. Disable obviously invalid actions in the UI, but always handle command rejection.
- Keep this document current when commands, fields, enum values, queue transitions, or platform support change. Update the table above and the matching TypeScript service in the same change.

## Source map

- Command registration: [`src-tauri/src/lib.rs`](../../src-tauri/src/lib.rs)
- Command wrappers: [`src-tauri/src/commands.rs`](../../src-tauri/src/commands.rs)
- Game and settings models: [`src-tauri/src/database.rs`](../../src-tauri/src/database.rs)
- Steam scan/import: [`src-tauri/src/steam_local.rs`](../../src-tauri/src/steam_local.rs), [`src-tauri/src/steam_import.rs`](../../src-tauri/src/steam_import.rs)
- Manual executable selection: [`src-tauri/src/manual_import.rs`](../../src-tauri/src/manual_import.rs)
- Catalog/source: [`src-tauri/src/catalog.rs`](../../src-tauri/src/catalog.rs), [`src-tauri/src/legio_source.rs`](../../src-tauri/src/legio_source.rs)
- Queue/finalization: [`src-tauri/src/download_queue.rs`](../../src-tauri/src/download_queue.rs), [`src-tauri/src/finalize_install.rs`](../../src-tauri/src/finalize_install.rs)
- Launch/account matching: [`src-tauri/src/game_lifecycle.rs`](../../src-tauri/src/game_lifecycle.rs), [`src-tauri/src/steam_switch.rs`](../../src-tauri/src/steam_switch.rs)
- Current frontend services: [`src/lib/services`](../../src/lib/services)
