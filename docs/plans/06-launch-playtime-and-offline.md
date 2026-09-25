# Phase 06: Launch, playtime, and offline

## Status

In progress

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

Status: Steam-managed launch, account overrides, and a native Windows launch backend are implemented. Windows runtime verification remains.

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

- Persist detected game sessions and derive per-game playtime aggregates. Backend storage, crash recovery, and summaries are implemented; connecting them to Home and Library remains UI work.
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
- Native Windows process association requires ancestry from the launched process, executable paths beneath its directory, and creation time after the launch request. This prevents Stop from targeting unrelated processes started in the same directory. If the launcher exits before a child appears, Legio waits three seconds for the handoff and then reports a launch error. The running-state exit grace also covers gaps up to three seconds. Verify longer helper chains on a real Windows game before closing 06B.
- Linux exposes no verified runtime account identity in this integration. The account override therefore uses the persisted login selection as its startup signal; it must not be described as proof of successful authentication.

## Accepted session policy

- A session begins when the lifecycle monitor first detects the game's process, and ends after the process has exited and the existing three-second exit grace period completes.
- Multiple games may have active sessions at once. Playtime is summed independently for each game, so time across different games can overlap.
- The backend writes a heartbeat every 30 seconds. On next startup, any still-open session is recovered as interrupted and ends at its last heartbeat. This can undercount by up to one heartbeat interval, but does not add time after the last observed game process.
- Stored timestamps and aggregate playtime use milliseconds. Session persistence errors are exposed in the launch state while process monitoring continues.

## Update notes

The launch runner now accepts a launch action while retaining the same process monitor and terminate path. A Linux integration test prepares a target, starts a controlled `sleep` process with the Steam App ID environment, observes `Running`, stops it, and verifies the final `Idle` state. A missing executable test verifies that launch errors include the failing stage. This covers the runner with a controlled process; it does not exercise database-backed preparation, the Steam account-switch flow, or Windows process tracking, so 06A remains in progress.

[PR #18](https://github.com/fraa2a/Legio/pull/18) prepares the Rust backend for per-game Steam account preferences: bounded local account enumeration, SQLite persistence, and a fail-closed prelaunch check. It does not add account controls to the testing UI or launch a game. A trustworthy active-account source and actual Steam launch integration remain required before this feature can be enabled.

[TcNo Account Switcher documents a community-used flow based on `loginusers.vdf`, the Windows `AutoLoginUser` registry value, and restarting Steam after it exits](https://github.com/TCNOco/TcNo-Acc-Switcher/wiki/Platform-Steam). Linux switchers use `~/.steam/registry.vdf` for the corresponding value. Steam client updates have changed the login flag names, so Legio must recognize observed variants and preserve unmodified content. This is best-effort integration with undocumented local state, not a Valve-supported API. [Steamworks documentation](https://partner.steamgames.com/doc/sdk/api) describes `steam://run/<AppID>` for launching through the currently active Steam account, and [Steam Support](https://help.steampowered.com/faqs/view/7EFD-3CAE-64D3-1C31) explains saved sign-in credentials.

[PR #21](https://github.com/fraa2a/Legio/pull/21) adds the testing UI and Steam account switching. The launch flow now starts Steam explicitly when needed, sends `-applaunch <AppID>`, and exposes Rust-owned `launching`, `running`, and `idle` states for each game. The testing UI offers Cancel during launch and Stop while a game process is running. Linux game detection uses the Steam App ID process environment and excludes Steam and Proton launch helpers; Windows detection uses the executable path under the installed game directory. A cancelled request that Steam has already accepted is watched for a late game process and stops that process if it appears within the launch timeout. Runtime account verification on Linux remains limited to Steam's persisted selection, and Steam may still require sign-in or Steam Guard.

Cancel also observes Steam's launch task and per-App-ID process events. When Steam has completed the launch task and reports no active game process, Legio waits a short grace period with no detected executable before returning to Play. If the executable appears, Legio stops it and waits for its exit. Missing terminal events retain the bounded launch timeout and report an unresolved Steam launch when appropriate.

Local Steam `config.vdf` can contain literal newlines in quoted values. The VDF parser accepts those values while preserving unrelated bytes, and the account switch validates candidate patches in memory before requesting shutdown. File changes occur only after Steam exits; the source files are checked again before backup and atomic replacement. During launch, a Steam refusal recorded for the requested App ID in new `console_log.txt` entries ends `launching` with a sanitized error. If the log is unavailable and no game process appears, the normal launch timeout still applies.

Some Linux `loginusers.vdf` files omit `MostRecent` for every saved account. A unique `AutoLogin=1` is accepted as the persisted account selection in that case, avoiding an unnecessary Steam restart when it matches the game's override. Conflicting or ambiguous flags leave the selection unknown. This still does not independently verify the authenticated runtime account.

Phase 05C stores manually selected executable paths separately from Steam installations. Steam-managed launch and account selection work for Steam-owned games; native executable launch, Home and Library integration for persisted sessions, and the full offline surfaces remain Phase 06 work.

The backend now stores a session when the monitor detects a Steam-managed or compatibility-runner game process, updates its heartbeat while it runs, closes it when monitoring ends, and recovers open sessions after restart at the last heartbeat. `get_playtime_summaries` returns per-game milliseconds and active-session counts. Home and Library presentation is intentionally outstanding because the UI is excluded from this work.
