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
- Before a game with an account selection launches, verify the active Steam account against the selected ID. Launch through Steam only on a verified match. On mismatch, a missing selected account, or uncertain active identity, return an actionable error and leave the game unlaunched. Recheck after the user changes accounts in Steam.
- Never switch accounts by passing credentials, editing Steam account files, or terminating Steam or running games. Do not silently launch under whichever account is active.
- Launch native Windows games with structured arguments and working directory.
- Track actual game lifetime rather than short-lived helpers.

Verification: Steam ownership remains intact and helper-exit scenarios do not prematurely end monitoring. Fixture tests cover missing or malformed account metadata, duplicate display names, stale selections, matching and mismatched active IDs, unknown active identity, and no sensitive fields in errors or logs. Native Linux and Windows checks show that account-specific launch proceeds only after the active Steam ID is verified as the selected ID. Running games are not stopped by Legio.

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
- Validate a trustworthy way to read the active desktop Steam account ID on Linux and Windows before enabling account-specific launch. A locally saved account list or a last-used marker does not prove current identity. If identity cannot be verified, keep account-specific launch unavailable with a clear error.
- Define session crash recovery and overlap rules before 06C.

## Update notes

No Phase 06 implementation updates yet. This account selection requirement prepares the backend for a later per-game override and does not add Legio accounts or a UI in this phase document.

[Valve's September 2026 Steam client update](https://steamcommunity.com/app/593110/announcements/) says Change Account opens Steam's account chooser and restarts Steam as the chosen account. It warns that changing accounts while a game is running can close active games and says Steam waits for just-exited games to finish cloud sync. [Steamworks documentation](https://partner.steamgames.com/doc/sdk/api) describes `steam://run/<AppID>` and requires a license on the currently active Steam account. Therefore, a game launch URI alone does not select a saved account. [Steam Support](https://help.steampowered.com/faqs/view/7EFD-3CAE-64D3-1C31) says saving sign-in credentials is optional. No supported automatic account-selection command or trustworthy active-account check has been established for Legio. A local Steam account metadata file was observed with display-name fields, but its format is not treated as a supported account-switching interface.
