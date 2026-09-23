# Phase 06: Launch, playtime, and offline

## Status

Planned

## Canonical requirements

Owns `PLAN.md` sections 23, 24, and 27, plus Home and local Library behavior from section 4.

## Objective

Provide one reliable launch lifecycle across ownership types, persist actual play sessions, and preserve local product behavior without network access.

## Scope

Shared launch lifecycle, Steam-managed launch, optional per-game Steam account selection, native Windows process tracking, session storage and aggregation, Home, local Library, launch configuration and errors, Retry, and offline waiting semantics.

## Milestones and principal tasks

### 06A: Shared launch lifecycle

- Define Rust-owned `prepare`, `launch`, `monitor`, `terminate`, and `collect diagnostics` stages.
- Represent stage failures as typed, actionable results.
- Keep platform-specific behavior behind the shared lifecycle only when needed.

Verification: a controlled executable traverses every stage, termination is observed, and stage failures expose useful diagnostics.

### 06B: Steam and native Windows launch

- Launch Steam-managed games through Steam.
- Enumerate locally saved Steam account IDs and display names using bounded, read-only parsing of Steam-owned local metadata. Do not expose login names, passwords, tokens, or the raw source file.
- Persist an optional per-game Steam account ID in the local database without changing the game's Steam App ID, internal ID, or manual overrides. A missing or renamed local account must not silently change that selection.
- In each Steam game's override settings, provide a toggle for account selection. When enabled, show a dropdown of locally saved account display names backed by SteamID64 values. Distinguish duplicate names, keep a missing selection visible, and require a selected account before saving the enabled override.
- If Steam is open under another account, ask before closing and restarting it. Use a best-effort local account switch: request a graceful Steam shutdown, wait for every Steam client process to exit, back up and atomically update only the account-selection values in Steam's local VDF files, and set the Windows registry value where applicable. Preserve unknown fields and never pass credentials. Start Steam and launch the game after the platform account signal matches the selection. On Windows this uses Steam's `ActiveProcess\\ActiveUser`; on Linux it can only confirm the persisted auto-login selection, not prove that Steam completed authentication. Steam may still request a password or Steam Guard. Cancellation, timeout, malformed state, or a mismatched signal leaves the game unlaunched. This relies on undocumented Steam client state and can break after a Steam update.
- If Steam requests a password or Steam Guard because its saved session is missing or expired, let Steam handle that normally; Legio must not collect, store, or pass credentials.
- Launch native Windows games with structured arguments and working directory.
- Track actual game lifetime rather than short-lived helpers.

Verification: Steam ownership remains intact and helper-exit scenarios do not prematurely end monitoring. Fixture tests cover missing or malformed account metadata, duplicate display names, stale selections, matching and mismatched active IDs, unknown active identity, confirmation cancellation, switch failure, backups, rollback, unknown-field preservation, and no sensitive fields in errors or logs. Native Linux and Windows checks cover saved-session, password/Steam Guard, and active-account verification. Steam and running games remain open when confirmation is declined. Steam account-file formats and switch behavior require revalidation after client updates.

### 06C: Sessions, product surfaces, and offline behavior

- Persist sessions and derive playtime aggregates.
- Populate Home and local Library from stored data.
- Expose launch configuration, errors, and real Retry behavior.
- Keep network tasks waiting rather than retrying in a loop.
- Preserve Home, Library, local metadata, playtime, settings, and permitted installed-game launch offline.
- Mark remote Store, source, download, and update work unavailable offline.

Verification: session totals survive restart, local surfaces remain useful offline, permitted games launch, and remote work waits without request churn.

## Dependencies

Phase 05 supplies installed game records and executable selection. Phase 03 supplies shared connectivity state.

## Completion criteria

Supported games launch through the correct owner, actual game lifetime drives sessions, playtime persists, local product surfaces work offline, and remote operations wait clearly.

## Open technical decisions

- Define helper-to-game process association rules before 06B.
- Linux exposes no verified runtime account identity in this integration. The account override therefore uses the persisted login selection as its startup signal; it must not be described as proof of successful authentication.
- Define session crash recovery and overlap rules before 06C.

## Update notes

[PR #18](https://github.com/fraa2a/Legio/pull/18) prepares the Rust backend for per-game Steam account preferences: bounded local account enumeration, SQLite persistence, and a fail-closed prelaunch check. It does not add account controls to the testing UI or launch a game. A trustworthy active-account source and actual Steam launch integration remain required before this feature can be enabled.

[TcNo Account Switcher documents a community-used flow based on `loginusers.vdf`, the Windows `AutoLoginUser` registry value, and restarting Steam after it exits](https://github.com/TCNOco/TcNo-Acc-Switcher/wiki/Platform-Steam). Linux switchers use `~/.steam/registry.vdf` for the corresponding value. Steam client updates have changed the login flag names, so Legio must recognize observed variants and preserve unmodified content. This is best-effort integration with undocumented local state, not a Valve-supported API. [Steamworks documentation](https://partner.steamgames.com/doc/sdk/api) describes `steam://run/<AppID>` for launching through the currently active Steam account, and [Steam Support](https://help.steampowered.com/faqs/view/7EFD-3CAE-64D3-1C31) explains saved sign-in credentials.

[PR #21](https://github.com/fraa2a/Legio/pull/21) adds the testing UI and Steam account switching. The launch flow now starts Steam explicitly when needed, sends `-applaunch <AppID>`, and exposes Rust-owned `launching`, `running`, and `idle` states for each game. The testing UI offers Cancel during launch and Stop while a game process is running. Linux game detection uses the Steam App ID process environment and excludes Steam and Proton launch helpers; Windows detection uses the executable path under the installed game directory. A cancelled request that Steam has already accepted is watched for a late game process and stops that process if it appears within the launch timeout. Runtime account verification on Linux remains limited to Steam's persisted selection, and Steam may still require sign-in or Steam Guard.
