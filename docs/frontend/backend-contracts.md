# Frontend and backend contracts

This is the implementation guide for wiring the Legio UI to the Rust/Tauri backend. Keep it aligned with the registered commands in [`src-tauri/src/lib.rs`](../../src-tauri/src/lib.rs), their request and response types, and the frontend service modules under [`src/lib/services`](../../src/lib/services).

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

### Manual executable import

| Command | Arguments | Result |
| --- | --- | --- |
| `scan_game_executables` | `{ directory, gameName? }` | `{ candidates: [{ path, score, signals }], selectedPath }` |
| `import_manual_game` | `{ input: { executablePath, name? } }` | created `Game` |
| `set_game_executable` | `{ gameId, executablePath }` | updated `Game` |

Present all candidates when `selectedPath` is null. `selectedPath` is a suggestion, not a substitute for a user's explicit choice when the scan is ambiguous. A successful import adds a manual game with a null `steamAppId` and a populated `executablePath`.

### Per-game Steam account override

| Command | Arguments | Result |
| --- | --- | --- |
| `list_saved_steam_accounts` | none | `{ accounts: [{ steamId, displayName }], diagnostics }` |
| `set_game_steam_account_preference` | `{ gameId, steamId: string | null }` | updated `Game`; null clears the override |
| `check_game_steam_account` | `{ gameId }` | `{ status, accountRequirementMet, selectedSteamId, message }` |

Do not expect `accountName` in the account list. It is intentionally private to the backend. Account check status is `not_required`, `missing_saved_account`, `match`, `mismatch`, or `unknown`. Treat `unknown` as inconclusive and display `message` when present.

Before launching an overridden Steam game, call `inspect_steam_game_launch({ gameId })`. It returns `{ status, steamRunning, currentAccountName, targetAccountName }`, with status `no_override`, `already_matches`, `mismatch`, or `unknown`. If status is `mismatch`, ask whether Steam may be closed and restarted, then call `launch_steam_game({ gameId, confirmAccountSwitch: true })` only after confirmation. For other statuses call with `false`. An `unknown` account must not be presented as a verified match.

### Compatibility settings for Linux manual games

These commands are in PR #35 and become available with that backend change. Do not wire them against `main` until the PR is merged.

| Command | Arguments | Result |
| --- | --- | --- |
| `get_compatibility_defaults` | none | global defaults |
| `save_compatibility_defaults` | `{ defaults }` | saved defaults |
| `get_game_compatibility_overrides` | `{ gameId }` | nullable per-game values |
| `save_game_compatibility_overrides` | `{ gameId, overrides }` | saved overrides |
| `list_compatibility_runners` | none | `{ runners, diagnostics }` |
| `launch_configured_game_with_runner` | `{ gameId }` | starts configured manual game |

Defaults have `runnerPath`, `prefixRoot`, `argumentsBefore`, `argumentsAfter`, `workingDirectory`, `environment`, and `dllOverrides`. Per-game overrides have those settings plus `prefixPath`. For scalar/list values, `null` inherits; an empty string/list clears an inherited value. Environment and DLL maps merge by key; an empty map clears all inherited entries. Saving settings persists them, but the UI should not imply that settings were applied until the save command succeeds.

`launch_game_with_runner({ gameId, runnerPath })` is an explicit-runner testing command. Production UI should use `launch_configured_game_with_runner` after selecting settings. Runner discovery and configured launch are Linux-only; on other platforms discovery returns no runners and a diagnostic.

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

Search the cache as the immediate response, debounce user input, and use `refresh_catalog` for network refresh. A failed refresh rejects; keep the cached results visible and show the error. Catalog errors have `kind` values `invalid_query`, `invalid_response`, `timeout`, `network`, `http`, `too_large`, `database`, or `internal`. Catalog results are not automatically added to the library. To add an entry, either create a Steam-linked game or download a Legio source entry and finalize its install.

Availability is joined by Steam App ID from the Legio manifest. `verified` and `unverified` indicate source trust, `unavailable` means no downloadable Legio release exists, and `unknown` means source status could not be established.

### Legio source

| Command | Arguments | Result |
| --- | --- | --- |
| `get_legio_source` | none | `{ manifest, cachedAt, stale, warning }` |
| `refresh_legio_source` | none | same shape |

Manifest v1 fields are `schemaVersion`, `generatedAt`, `verified[]`, and `unverified[]`. Each entry contains `steamAppId`, `name`, `release: { version, publishedAt }`, and `download: { url, sha256, sizeBytes }`. Use source entries only through `queue_download`; do not download directly from a URL in the frontend. If refresh fails but a valid cache exists, the backend returns the stale cache and a warning.

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
| `set_download_bandwidth_limit` | `{ bytesPerSecond }` | `void`; zero means unlimited |
| `stage_download` | `{ id }` | staged directory path |
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

`queue_download` requires `acceptUnverified: true` before an unverified source can be queued. Poll `list_downloads` while the queue screen is visible because there is no progress event yet. Refresh the library after a successful `finalize_download`. For install selection, stage the verified archive, scan the staged directory, let the user select the executable, then pass its path relative to the staged root as `executableRelative`. Never use an executable outside the staged directory. If the UI cannot safely derive this relative path on every platform, add a backend command returning relative candidates before shipping the finalization flow.

## Launch lifecycle

| Command | Arguments | Result |
| --- | --- | --- |
| `launch_steam_game` | `{ gameId, confirmAccountSwitch }` | `{ gameId, steamAppId }` accepted launch request |
| `launch_game_with_runner` | `{ gameId, runnerPath }` | `void`, testing path |
| `launch_configured_game_with_runner` | `{ gameId }` | `void`, configured manual game launch |
| `list_game_launch_states` | none | `[{ gameId, status, error? }]` |
| `cancel_game_launch` | `{ gameId }` | `void` |
| `stop_game` | `{ gameId }` | `void` |

Launch state status is currently `idle`, `launching`, or `running`. The launch command returning successfully means launch was requested, not that the game process is running. Poll state while a game is launching or running. Show `cancel` while `launching`, call `cancel_game_launch`, then reconcile to idle/error from the state list. Show `stop` only when running and call `stop_game`. Surface the state's `error` and rejected command errors. There is no `cancelling` backend state in this contract, so if the UI displays one, treat it as transient local presentation until the next backend state confirms the outcome.

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
