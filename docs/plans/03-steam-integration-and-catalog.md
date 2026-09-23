# Phase 03: Steam integration and catalog

## Status

Completed

## Canonical requirements

Owns `PLAN.md` sections 5, 7, 19, 30, 31, and 33.

## Objective

Detect local Steam installations and games, maintain a searchable local catalog, and establish bounded networking, caching, connectivity, cancellation, and diagnostics before remote catalog use.

## Scope

Centralized HTTP behavior, local logs, Steam VDF and ACF discovery, imported library entries, separately indexed Steam catalog data, and on-demand metadata and artwork caching.

## Milestones and principal tasks

### 03A: Networking, connectivity, cache, and diagnostics

Status: Completed.

- Build one Rust HTTP client with timeouts, cancellation, bounded responses, and typed errors.
- Track connectivity without aggressive retry loops.
- Store cache metadata and actionable local logs without secrets.

Verification: timeout, cancellation, offline, oversized response, cache hit, and cache miss scenarios produce bounded, typed, diagnosable outcomes.

Evidence: [PR #11](https://github.com/fraa2a/Legio/pull/11) added the shared HTTP client and catalog cache. [PR #13](https://github.com/fraa2a/Legio/pull/13) added the Steam details cache. [PR #15](https://github.com/fraa2a/Legio/pull/15) added bounded local network logs and the diagnostics status UI. The [PR #15 Rust CI job](https://github.com/fraa2a/Legio/actions/runs/35775565439/job/106907767635) passed formatting, Clippy with warnings denied, and all Rust tests. Its [frontend job](https://github.com/fraa2a/Legio/actions/runs/35775565439/job/106907767346) and [Linux](https://github.com/fraa2a/Legio/actions/runs/35775565439/job/106907767822) and [Windows](https://github.com/fraa2a/Legio/actions/runs/35775565439/job/106907767568) Tauri build smoke jobs also passed. Tests cover timeout, connection failure, request cancellation, HTTP failure, declared and streamed oversize responses, local cache hit and miss, log rotation, queue saturation, and write failure. A Windows build smoke does not prove Windows Steam discovery.

### 03B: Steam installation and library detection

Status: Completed.

- Discover supported Steam locations on Windows and Linux.
- Parse VDF and ACF metadata as untrusted input.
- Import installed Steam games using the Phase 02 identity model.
- Preserve later launch through Steam rather than direct executable substitution.

Verification: fixture and local-install scenarios discover libraries and imports deterministically while malformed metadata reports useful errors.

Evidence: [PR #12](https://github.com/fraa2a/Legio/pull/12) added bounded VDF and ACF parsing, Linux default locations, and a transactional import that preserves internal IDs and manual name overrides. [PR #16](https://github.com/fraa2a/Legio/pull/16) added Windows default roots, startup import, and local `appinfo.vdf` type classification that excludes Steam tools without a name blacklist. Its [CI run](https://github.com/fraa2a/Legio/actions/runs/35862783594) passed Rust and frontend checks, a focused Windows discovery test, and Linux and Windows Tauri builds. A real local Steam appinfo file with 712 records was parsed; a known game was classified `Game` and Proton was classified `Tool`. Entries without a verified type are skipped with diagnostics. The user observed the native window opening and showing Steam games before the filter fix. A post-fix native import and native Windows scan have not yet been observed. Steam-managed launch remains Phase 06 work.

### 03C: Steam catalog and media cache

Status: Completed.

- Keep the catalog index separate from the user library.
- Index search data locally so typing never depends on a live request.
- Fetch metadata and artwork on demand through the shared client.
- Apply bounded storage and cache metadata rules.

Verification: local search works offline after indexing, uncached details load on demand, and stale or unavailable artwork degrades clearly.

Evidence: [PR #11](https://github.com/fraa2a/Legio/pull/11) stores Hydra search records separately from the library and searches that local cache while typing. [PR #13](https://github.com/fraa2a/Legio/pull/13) fetches and caches Steam Store details and validated Steam image URLs on demand. [PR #17](https://github.com/fraa2a/Legio/pull/17) fetches selected image bytes through Rust, validates them, and keeps a bounded local cache with stale offline fallback. The [PR #17 CI run](https://github.com/fraa2a/Legio/actions/runs/35861225666) passed Rust, frontend, Linux, and Windows jobs. The catalog cache is populated only by explicit, bounded Hydra searches, so it remains a partial catalog.

## Dependencies

Phase 02 supplies persistent state, settings, game identity, and local library operations.

## Completion criteria

Steam installations and games are detected, imports retain Steam ownership, catalog search is local while typing, media is cached on demand, and network failures are bounded and diagnosable.

## Provider and cache decisions

- Hydra Launcher API supplies catalog search through anonymous `POST https://hydra-api-us-east-1.losbroxas.org/catalogue/search`. Each explicit refresh requests at most 50 Steam records. Typing searches only the accumulated local cache, capped at 20,000 records and 100 displayed matches. Catalog data does not indicate Legio download availability.
- The public Steam Store `GET https://store.steampowered.com/api/appdetails?appids=<id>&l=english` supplies details for one App ID and Steam image URLs. The details cache holds at most 128 entries of 64 KiB each, with a 24-hour freshness period and at most eight screenshots per entry. Selected raster image bytes are limited to 2 MiB each and cached in at most 64 local files, with a 24-hour freshness period and stale offline fallback.
- The Rust HTTP client has a 5-second connection timeout, 10-second request timeout, at most three redirects, and a 2 MiB response limit for Hydra search and Steam details. Local network JSONL logs rotate at 256 KiB with a 64-record queue and omit URLs, request bodies, paths, and raw error text.
- Live smoke reported in the [Phase 03 handoff](../handoffs/phase03-continuation.md) observed 50 Hydra results from 110 matches for a Portal query and a 12,687-byte Steam `appdetails` response for App ID 400. These observations inform the current caps but do not establish maximum provider payload sizes.
- The approved download source is a separate Legio-hosted versioned JSON file, documented by [PR #14](https://github.com/fraa2a/Legio/pull/14), whose [Rust CI job](https://github.com/fraa2a/Legio/actions/runs/35746959687/job/106810953747) passed. Its parser and download integration belong to Phases 04 and 05.
- [Hydra Launcher's published Terms of Use](https://github.com/hydralauncher/hydra-legal/blob/main/terms-of-use.md) describe platform use and may change. They do not expressly document this catalog API endpoint or third-party API access rights.
- The [Steamworks Web API overview](https://partner.steamgames.com/doc/webapi_overview) describes public methods on `api.steampowered.com` and protected methods that require authentication. Legio's `appdetails` URL is a Steam Store API endpoint on `store.steampowered.com`, not an endpoint documented by that overview. No third-party Store API access right is inferred from it.

## Update notes

The current Svelte verification surface exposes controls backed by Tauri commands for scan/import, catalog search, Steam details, image cache, connectivity, and diagnostics. Browser checks covered missing-Tauri-bridge failures. `pnpm tauri dev` compiled and opened a native window. On 2026-09-23, the user directly observed games in that window and the Proton/tool misclassification that PR #16 later fixed. The agent could not obtain a reliable WebView capture on this Wayland session. The user's observation confirms native discovery; a post-fix native import, Hydra refresh, and Steam details refresh still need observable evidence. The verification surface is not the final Browse/Store product UI.
