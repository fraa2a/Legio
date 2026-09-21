# Legio Launcher — Development Plan

## 1. Project definition

**Legio Launcher** is a desktop game launcher and downloader for **Windows and Linux**.

Its core goals are:

- provide a lightweight, modern game-launcher UI;
- detect and launch installed Steam games;
- expose the Steam catalog through a Browse/Store experience;
- match Steam games against a single remote Legio JSON source;
- download supported games from direct archive links;
- verify, extract, detect, install and add downloaded games to the library automatically;
- allow manual game import;
- run Windows games on Linux through a compatibility layer;
- reimplement in Rust the useful behavior and feature set of **Online Fix Linux Launcher**;
- remain fully usable offline for already-installed games.

Initial stack:

- **Tauri 2**
- **Rust**
- **Svelte 5**
- **TypeScript**
- **Vite**
- **Tailwind CSS 4**
- **SQLite**

Rust owns business logic, filesystem access, processes, networking, downloads, installation, Steam integration and OS integration.

Svelte owns presentation, interaction and frontend state.

The codebase must use an organized, modular and maintainable Rust architecture suitable for long-term development. Avoid large monolithic modules, UI-owned business logic and tight coupling between unrelated subsystems.

---

# 2. Product principles

Legio must prioritize:

- low resource usage;
- responsive UI;
- predictable behavior;
- clear user control;
- offline-first library access;
- recoverable downloads and installations;
- safe handling of remote data and archives;
- maintainability over short-term shortcuts;
- platform-specific behavior isolated behind clean abstractions;
- user-editable automatic detection results.

Legio does **not** require an account for its core functionality.

No telemetry is planned.

No plugin system is planned.

Legio uses **one remote source document controlled by the project**.

---

# 3. Supported platforms

Primary platforms:

- Windows
- Linux

Windows games must be supported on Linux through a compatibility system based on Proton/Wine.

Initial development may focus mainly on Windows-native games and running those games through compatibility on Linux.

Native Linux game support can be expanded later without redesigning the core data model.

---

# 4. Main application sections

The application must contain the following primary sections.

## Home

Personal activity dashboard.

Expected content:

- Continue Playing;
- recently played games;
- most played games;
- total recent playtime;
- recent downloads;
- monthly activity heatmap;
- optional summary statistics.

The activity heatmap should resemble a GitHub contribution grid:

- one cell per day;
- approximately the last 30 days;
- no playtime = empty/lowest intensity;
- greater playtime = progressively stronger intensity.

All playtime statistics must remain available offline.

## Library

Contains games available to the user from any supported origin:

- detected Steam games;
- games installed through Legio;
- manually added games.

Expected functionality:

- grid/list browsing;
- local search;
- sorting;
- filtering;
- recently played;
- favorites;
- playtime;
- installation state;
- game properties;
- remove from library;
- locate files;
- edit metadata and artwork;
- edit launch configuration.

## Browse / Store

Represents the Steam catalog.

It must allow the user to:

- browse available games;
- search games;
- open a game page;
- view Steam metadata and artwork;
- see whether a Legio download is available.

If the Steam App ID exists in the Legio source, a download can be offered.

If no matching Legio entry exists, the page must explicitly state that the download is unavailable.

## Downloads

Shows:

- queued downloads;
- active download;
- progress;
- speed;
- ETA;
- verification state;
- extraction state;
- failures;
- retry controls;
- pause/resume;
- cancellation.

The queue must survive application restarts.

## Settings

Must expose at least:

- games directory;
- temporary/download directory;
- update behavior;
- start-up behavior;
- Steam path;
- automatic Steam detection;
- automatic library scanning;
- Linux compatibility defaults;
- Proton configuration;
- theme;
- bandwidth limits;
- desktop notifications.

---

# 5. Global Store Search

Legio must provide a search button in the sidebar that opens a Spotlight-style overlay.

This is **not** a general command palette.

Its only purpose is game discovery/search.

Behavior:

1. open overlay;
2. search the indexed Steam catalog;
3. show matching games;
4. selecting a result opens its Store/Game page.

The feature should feel immediate and keyboard-friendly.

The search should operate on a locally indexed catalog whenever possible rather than querying the network for every keystroke.

---

# 6. Game identity

The canonical external identifier for recognized games is:

**Steam App ID**

Legio should also maintain an internal game identifier so manually added or unidentified games can exist without a Steam association.

Conceptually:

```text
internal_game_id
steam_app_id: optional
```

When a manually added game is later identified as a Steam title, the existing library entry should be enriched rather than replaced.

Steam App ID is used for:

- Steam metadata;
- artwork;
- Store matching;
- source matching;
- installation association;
- executable metadata where available.

---

# 7. Steam integration

Legio must integrate with locally installed Steam.

## Local Steam detection

Detect:

- Steam installation;
- Steam library folders;
- installed games;
- Steam App IDs;
- installation directories.

Use Steam's local metadata such as:

- `libraryfolders.vdf`
- `appmanifest_*.acf`

Detected Steam games are automatically added to the Legio library.

Steam-managed games should normally be launched through Steam so Steam continues to manage:

- Proton configuration;
- launch options;
- overlay;
- cloud features;
- Steam-specific behavior.

## Steam catalog

Browse must represent the Steam catalog rather than only installed games.

Legio should maintain a local searchable catalog cache containing only the information required for discovery and lookup.

Detailed metadata should be fetched and cached when needed.

Useful metadata includes:

- App ID;
- name;
- icon;
- cover;
- hero/banner;
- screenshots;
- description;
- genres/tags;
- ratings/review information;
- release information;
- system requirements.

Network-facing Steam integration must be isolated behind a dedicated service/interface so changes in Steam APIs do not spread through the rest of the application.

---

# 8. Legio remote source

Legio uses **one remote JSON source file**.

The source enriches the Steam catalog with Legio download availability.

Steam answers:

> What games exist?

The Legio source answers:

> Which games can Legio download, and which releases are available?

The source must be cached locally.

If the remote source cannot be refreshed, the last valid cached source may be used where appropriate, while clearly distinguishing stale data when necessary.

---

# 9. Source trust model

The source file contains two independent top-level game lists:

- `verified`
- `unverified`

Both lists use the **same game schema**.

A game entry is classified by which list contains it.

Do **not** store `verified` and `unverified` download arrays inside each individual game.

A Steam App ID must belong to only one trust list at a time.

If a game becomes verified, it should be moved from `unverified` to `verified`.

## Verified

A verified entry has been reviewed by Legio staff according to the project's verification process.

UI wording should use **Verified**, not an absolute claim such as "Safe".

Recommended display:

> Verified by Legio Staff

Verification must never be presented as a guarantee that software cannot be harmful.

## Unverified

An unverified entry remains downloadable.

The UI must visibly mark it as unverified and warn the user before installation.

Example meaning:

> This download has not been verified by Legio staff. It may contain unwanted or harmful files. Continue only if you trust the source.

The user is allowed to continue after the warning.

---

# 10. Source JSON schema

The JSON format is part of the project contract and should be versioned.

Example:

```json
{
  "schema_version": 1,
  "verified": [
    {
      "steam_app_id": 123456,
      "releases": [
        {
          "version": "1.2.0",
          "url": "https://example.org/game-1.2.0.7z",
          "archive": "7z",
          "size": 1234567890,
          "sha256": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
        },
        {
          "version": "1.1.0",
          "url": "https://example.org/game-1.1.0.zip",
          "archive": "zip",
          "size": 987654321,
          "sha256": "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789"
        }
      ]
    }
  ],
  "unverified": [
    {
      "steam_app_id": 654321,
      "releases": [
        {
          "version": "2.0.0",
          "url": "https://example.org/other-game-2.0.0.rar",
          "archive": "rar",
          "size": 2222222222,
          "sha256": "1111111111111111111111111111111111111111111111111111111111111111"
        }
      ]
    }
  ]
}
```

Required top-level fields:

```text
schema_version
verified
unverified
```

Required game fields:

```text
steam_app_id
releases
```

Required release fields:

```text
version
url
archive
size
sha256
```

Supported archive values initially:

```text
zip
7z
rar
```

Archives are:

- direct HTTP/HTTPS downloads;
- single-part;
- not password protected.

No multipart support is required initially.

The schema should remain extensible so future fields can be added without breaking old clients.

Potential future release metadata may include:

- release date;
- changelog;
- minimum launcher version;
- mirrors;
- installation metadata;
- compatibility hints.

These should only be introduced when required.

---

# 11. Source validation and security

Treat the remote JSON as untrusted input.

Validate:

- schema version;
- JSON structure;
- field types;
- App IDs;
- URLs;
- archive formats;
- sizes;
- SHA256 values;
- duplicate entries;
- duplicate App IDs across trust lists;
- duplicate or conflicting release versions.

Use:

- HTTPS;
- sensible connection/read timeouts;
- maximum response size;
- bounded parsing;
- atomic cache replacement.

A failed refresh must never overwrite the last valid cached manifest.

The source must not be able to inject arbitrary executable behavior.

Do not allow manifest fields that directly execute:

- shell commands;
- PowerShell;
- Bash;
- arbitrary scripts;
- registry scripts;
- arbitrary post-install hooks.

Application behavior belongs to Legio code.

---

# 12. Download engine

Downloads must use a persistent queue.

Required behavior:

- queueing;
- pause;
- resume;
- HTTP Range resume when supported;
- retry;
- cancellation;
- persistent state across launcher restarts;
- download progress;
- speed;
- ETA;
- bandwidth limiting;
- network interruption recovery;
- SHA256 verification;
- automatic extraction after successful verification.

Conceptual state machine:

```text
Queued
  ↓
Downloading
  ↓
Verifying
  ↓
Extracting
  ↓
Detecting
  ↓
Installed
```

Additional states:

```text
Paused
Failed
Cancelled
WaitingForNetwork
```

The download engine must not depend on the Downloads UI being open.

---

# 13. Download completion actions

Add support as a planned feature for an action after the complete queue finishes.

At minimum:

- do nothing;
- shut down the computer;
- sleep/suspend where supported.

This belongs to the download orchestration layer, not frontend-only logic.

---

# 14. Installation pipeline

Installation should be transactional where practical.

Expected flow:

```text
download archive
      ↓
verify SHA256
      ↓
extract into staging directory
      ↓
validate extracted content
      ↓
detect executable/game
      ↓
move/finalize into selected games directory
      ↓
create/update library entry
```

Do not extract directly into the final game directory before verification.

If SHA256 does not match:

- abort;
- never install;
- show a clear error;
- remove or quarantine the invalid archive according to project policy.

If extraction or detection fails:

- keep the library database consistent;
- never leave an entry falsely marked as successfully installed;
- allow retry or manual recovery.

---

# 15. Archive handling

Supported initially:

- ZIP
- 7z
- RAR

No encrypted archives.

No multipart archives initially.

Archive extraction must defend against:

- `../` traversal;
- absolute-path extraction;
- symlink escape;
- malformed paths;
- unreasonable expansion;
- archive bombs;
- disk exhaustion;
- overwrite outside the staging/install scope.

Temporary and partial files must be recoverable and cleanable.

---

# 16. Automatic executable detection

Do not select the first `.exe` found.

Legio should use a strategy inspired by the strengths of Hydra's executable detection while implementing it natively in Rust.

Expected approach:

1. use known Steam/game executable metadata when available;
2. recursively scan the installation;
3. identify executable candidates;
4. remove known false positives;
5. rank remaining candidates;
6. automatically select only when confidence is sufficient;
7. ask the user when the result is ambiguous.

Common false-positive categories include:

- uninstallers;
- installers;
- redistributables;
- DirectX setup;
- Visual C++ setup;
- crash handlers;
- anti-cheat installers;
- helper processes;
- web helpers;
- runtime executables.

Preferred evidence order:

```text
explicit trusted game metadata
known Steam executable information
known executable mappings
heuristic candidate ranking
manual user selection
```

If ambiguity remains, do not guess.

Show the detected candidates and allow the user to choose.

The selected executable must remain editable later.

---

# 17. Automatic game identification

For Legio-downloaded games, the Steam App ID is already known from the source.

After extraction, use that App ID to associate:

- name;
- metadata;
- artwork;
- executable knowledge;
- Store page;
- library information.

For manually added games:

1. user selects an executable;
2. Legio attempts to identify the corresponding Steam game;
3. if identified, suggest metadata automatically;
4. if not identified, create a custom game entry.

Possible identification signals:

- executable filename;
- known game executable mappings;
- folder names;
- installation structure;
- executable metadata;
- known Steam associations.

Automatic results are suggestions, not immutable values.

---

# 18. Manual game import

Manual import begins by selecting the game executable.

Legio attempts automatic identification.

If identified, prefill:

- Steam App ID;
- game name;
- cover;
- hero/banner;
- icon;
- relevant metadata;
- recommended launch settings where available.

If not identified:

- use a placeholder image;
- create a custom title;
- allow manual metadata.

The user must always be able to override:

- name;
- executable;
- cover;
- hero/banner;
- icon;
- working directory;
- launch arguments;
- runner;
- Steam App ID association.

---

# 19. Artwork and metadata

Games associated with a Steam App ID should use Steam-derived metadata and artwork where available.

Expected assets:

- icon;
- cover;
- hero/banner;
- screenshots.

Assets should be cached locally.

For unidentified games:

- use placeholders;
- allow manual artwork selection.

Artwork should not be repeatedly downloaded if a valid local cache entry exists.

---

# 20. Game page

Each Store/Game page should support the information appropriate to the title.

Expected layout/content:

- hero/banner;
- cover;
- title;
- Play / Download / Update action;
- install/library state;
- rating/review information;
- genres/tags;
- description;
- screenshots;
- release information;
- system requirements;
- download availability;
- installed version;
- available version;
- trust status for Legio downloads.

Action rules:

```text
installed → Play
download available → Download
new version available → Update
no Legio source entry → Download unavailable
```

Remote descriptions/content must be sanitized before rendering.

---

# 21. Game updates

Each source release has its own version.

Legio must compare:

```text
installed version
available source version
```

Update policies:

- automatic;
- ask before updating;
- manual;
- never.

Allow per-game overrides.

Update operations should use the same verification and staging principles as initial installation.

A failed update must not unnecessarily destroy a known-working installation.

---

# 22. Legio application updates

Legio application updates must remain separate from:

- game updates;
- source refreshes.

Treat these as different systems with different failure handling.

The application update system should support signed or otherwise verified releases.

---

# 23. Offline mode

Core installed-game functionality must work offline.

Available offline:

- Home;
- Library;
- local metadata;
- playtime;
- settings;
- launching installed games where the game itself permits it.

Unavailable offline:

- Store/Browse remote content;
- source refresh;
- new downloads;
- update checks requiring network.

If offline, Store/Browse should not repeatedly attempt failing requests.

Display a dedicated state such as:

```text
No internet connection

[ Retry ]
```

Retry should verify connectivity and restore network-backed functionality if available.

Queued network tasks should transition to a waiting state rather than continuously fail.

---

# 24. Playtime tracking

Legio must track game sessions.

Store at least:

```text
game
start time
end time
duration
```

Use this for:

- Continue Playing;
- Recently Played;
- most played games;
- total playtime;
- monthly activity;
- per-day heatmap.

Playtime data is local and must remain usable offline.

Game process tracking should be reliable enough to avoid counting launcher/helper processes as the actual game session.

---

# 25. Linux compatibility system

Linux support is a core requirement.

Legio must be able to run Windows games through a dedicated compatibility subsystem.

It should support the responsibilities required for a Steam-like experience, including:

- Proton;
- GE-Proton;
- Wine where appropriate;
- runner discovery;
- runner management;
- per-game runner selection;
- prefix management;
- environment variables;
- DLL overrides;
- launch arguments;
- working directories;
- Steam Runtime integration;
- overlay-related configuration;
- graphical compatibility options;
- diagnostics;
- per-game compatibility overrides.

The subsystem should have clear boundaries from generic game/library logic.

Windows launch behavior and Linux compatibility behavior should share common high-level launch concepts without mixing platform-specific implementation details.

---

# 26. Online Fix Linux Launcher reimplementation

Legio must reimplement the useful feature set of **Online Fix Linux Launcher** in Rust.

This is a rewrite, not a line-by-line translation.

The implementation should first identify each responsibility and redesign it into maintainable Legio subsystems.

Feature parity should cover relevant behavior including:

- Proton/GE-Proton discovery;
- Proton management;
- default Proton selection;
- per-game Proton selection;
- Proton prefix creation and management;
- custom prefix paths;
- Steam Runtime selection;
- environment configuration;
- Wine DLL overrides;
- arguments before/after the executable;
- Steam presence detection;
- starting Steam when required;
- Steam overlay integration where applicable;
- Wine/Proton process handling;
- game process monitoring;
- compatibility-specific graphics options;
- WineD3D options;
- Wayland-related options where applicable;
- game-specific settings;
- debug mode;
- process logs;
- Wine/Proton diagnostics;
- shortcuts;
- icon handling;
- supported game/fix-specific compatibility behavior.

The implementation should preserve Legio's architecture rather than reproduce architectural limitations of the original project.

---

# 27. Runner model

Legio should use a clean abstraction for launching games through different mechanisms.

Expected runner categories include:

- Steam-managed launch;
- native Windows launch;
- Proton launch;
- Wine-compatible launch where required.

The launcher should expose a common lifecycle concept such as:

```text
prepare
launch
monitor
terminate
collect diagnostics
```

Do not let Linux-specific behavior leak throughout unrelated game-management code.

---

# 28. Compatibility configuration

Provide global defaults and per-game overrides.

Global examples:

- default Proton;
- prefix root;
- Steam Runtime behavior;
- default environment;
- default graphics compatibility settings.

Per-game overrides:

- runner;
- Proton version;
- prefix;
- environment variables;
- DLL overrides;
- launch arguments;
- working directory;
- overlay;
- graphics options.

User overrides should take priority over automatically suggested values.

---

# 29. Persistence

Use SQLite for persistent application state.

Persist information such as:

- settings;
- library games;
- Steam associations;
- installations;
- manually overridden metadata;
- downloaded release/version state;
- download queue;
- source cache metadata;
- Steam metadata cache;
- compatibility settings;
- per-game launch configuration;
- play sessions;
- aggregated playtime.

Use versioned database migrations from the beginning.

Do not couple persistent storage directly to UI components.

---

# 30. Cache

Separate structured metadata from large assets.

Structured/cacheable data may include:

- Steam catalog entries;
- Steam metadata;
- source data;
- derived indexes.

Disk asset cache may include:

- covers;
- heroes/banners;
- icons;
- screenshots.

Use HTTP caching mechanisms where useful, including:

- ETag;
- Last-Modified;
- TTL/expiry.

Avoid unnecessary repeated downloads.

---

# 31. Network architecture

Use a centralized network layer for:

- Steam requests;
- Legio source refresh;
- downloads;
- application update checks.

It should support:

- consistent timeouts;
- cancellation;
- retry policy;
- connectivity state;
- user-agent/version information;
- bounded response sizes where appropriate.

Do not let every UI component perform independent unmanaged network calls.

---

# 32. Error model

Use typed/structured internal errors.

Important categories include:

```text
NetworkUnavailable
SourceInvalid
SourceUnsupported
ChecksumMismatch
ArchiveCorrupt
ArchiveUnsafe
DiskFull
ExecutableNotFound
ExecutableAmbiguous
SteamUnavailable
RunnerUnavailable
ProtonUnavailable
LaunchFailed
DatabaseFailure
PermissionDenied
```

The UI should translate internal errors into useful messages.

Avoid generic failures when the actual problem is known.

---

# 33. Logging and diagnostics

Use structured local logging.

Useful categories:

- application;
- Steam;
- source;
- network;
- downloads;
- extraction;
- detection;
- library;
- launch;
- compatibility;
- database.

Compatibility diagnostics should make it possible to inspect:

- selected runner;
- Proton version;
- prefix;
- relevant environment;
- process exit code;
- stdout/stderr where appropriate.

Do not automatically upload logs.

Provide a way for the user to locate/open logs.

Do not log secrets or sensitive local information unnecessarily.

---

# 34. Frontend/backend communication

Keep Tauri IPC narrow and explicit.

The frontend should request high-level operations such as:

```text
get library
get game
search catalog
start download
pause download
launch game
update settings
```

Long-running backend work should report state through events rather than frontend polling loops.

Useful event categories:

```text
download progress
download state changed
extraction progress
library changed
game started
game stopped
network state changed
update available
```

---

# 35. UI direction

Use the supplied mockup as the primary visual direction.

Key characteristics:

- dark, polished interface;
- narrow sidebar;
- content-focused layouts;
- large game artwork;
- rounded cards/panels;
- restrained controls;
- prominent primary actions;
- clear hierarchy;
- minimal visual noise;
- modern game-launcher feel.

Hydra Launcher is a primary functional UX reference for:

- Store discovery;
- game search;
- game pages;
- library behavior;
- download presentation;
- future achievements.

The goal is inspiration and behavior parity where useful, not visual duplication.

---

# 36. Future achievement system

Achievements are part of the final product plan but are not required for the initial core implementation.

Target behavior should be comparable to Hydra where technically possible.

The subsystem should eventually support:

- achievement metadata;
- locked/unlocked state;
- unlock date;
- progress where available;
- notifications;
- per-game completion;
- local persistence;
- Steam-backed achievements where available;
- compatible detection methods for non-Steam-managed installations where technically possible.

Keep the achievement system modular because different games may require different detection mechanisms.

---

# 37. Future accounts

Accounts are not required for the initial product.

The core application must never require an account to:

- view local library;
- launch games;
- use compatibility features;
- manage local installations.

Future account-related capabilities may include:

- profile;
- cloud sync;
- cross-device settings;
- optional social features.

Account support should be additive rather than foundational to core launcher behavior.

---

# 38. Repository and engineering quality

The repository should be prepared as a serious long-term software project.

Maintain:

- clear contribution rules;
- code formatting;
- static analysis;
- tests;
- dependency update automation;
- security policy;
- CI;
- release automation.

Required CI targets should include Windows and Linux.

CI should validate at least:

- Rust formatting;
- Rust linting;
- Rust tests;
- frontend type checking;
- frontend linting;
- frontend build;
- Tauri build/smoke validation where practical.

Protect the main branch with required checks.

---

# 39. Release system

Automate release artifacts for:

- Windows;
- Linux.

Releases should include:

- packaged binaries/installers;
- checksums;
- appropriate metadata;
- GitHub release assets.

Support a path toward:

- development builds;
- prereleases;
- stable releases.

Self-update must remain independent from game updates.

---

# 40. Initial development objective

The first implementation should prove the architecture with a minimal but functional UI and Rust backend.

The first complete vertical slice should eventually perform:

```text
start Legio
   ↓
initialize database
   ↓
detect Steam
   ↓
import installed Steam games
   ↓
load/search Steam catalog
   ↓
match Steam App ID with Legio source
   ↓
download archive
   ↓
verify SHA256
   ↓
extract safely
   ↓
detect executable
   ↓
add game to library
   ↓
launch game
```

On Linux:

```text
launch game
   ↓
compatibility subsystem
   ↓
Proton/Wine configuration
   ↓
game process
```

The first UI does not need the final visual complexity.

Its purpose is to validate the complete architecture and core flows.

---

# 41. Development order

Recommended implementation order:

1. repository/tooling foundation;
2. application shell;
3. database and migrations;
4. settings;
5. core game/library model;
6. Steam local detection;
7. Steam library import;
8. Steam catalog/search;
9. Steam metadata/artwork cache;
10. Legio source parsing and validation;
11. source/cache merge with Steam catalog;
12. persistent download queue;
13. resume/retry/network handling;
14. SHA256 verification;
15. archive extraction/staging;
16. executable detection;
17. manual game import;
18. Windows launch/process handling;
19. playtime tracking;
20. Linux compatibility subsystem;
21. Online Fix Linux Launcher feature reimplementation;
22. Home;
23. Library;
24. Browse/Store;
25. Spotlight-style Store search;
26. Game page;
27. Downloads UI;
28. Settings UI;
29. game update system;
30. application update system;
31. complete offline/network-state behavior;
32. desktop notifications;
33. download-completion power actions;
34. achievements;
35. optional account system.

This order is a development path, not a rigid internal file/module layout.

---

# 42. Definition of done for the core product

The core launcher is functionally complete when it can:

- run on Windows and Linux;
- detect Steam and installed Steam games;
- import Steam games into Legio;
- search the Steam catalog;
- show Steam metadata and artwork;
- read the Legio JSON source;
- distinguish verified and unverified game entries;
- show clear unverified warnings;
- download supported archives;
- pause/resume/retry downloads;
- recover the queue after restart;
- verify SHA256 before installation;
- safely extract supported archives;
- identify the correct game executable or ask the user;
- automatically add successful installations to the library;
- add arbitrary games manually;
- enrich manual games when Steam identity is detected;
- allow users to override automatic metadata;
- launch games reliably;
- track playtime;
- provide Home and Library views;
- work offline for local functionality;
- run supported Windows games on Linux through the compatibility subsystem;
- reproduce the required Online Fix Linux Launcher capabilities in Rust;
- expose useful diagnostics when launch/compatibility fails.

---

# 43. Architectural boundaries

Keep these concepts independent:

```text
Steam Catalog
      │
      ├───────────────┐
      │               │
Legio Source      Steam Local Scanner
      │               │
      └──────┬────────┘
             ↓
          Library
             │
     ┌───────┴────────┐
     ↓                ↓
Downloads          Launcher
     │                │
Installation          ├─ Steam
Pipeline              ├─ Windows
                      └─ Linux Compatibility
                              │
                              └─ OFLL feature parity
```

Hydra should serve mainly as a reference for:

- library UX;
- Store UX;
- search;
- download presentation;
- executable detection;
- future achievements.

Online Fix Linux Launcher should serve mainly as a reference for:

- Proton/Wine behavior;
- prefixes;
- Steam Runtime;
- environment configuration;
- overlay integration;
- Linux-specific launch handling;
- compatibility features.

Legio should remain its own architecture rather than becoming a direct structural clone of either project.

---

# 44. Non-goals

Do not introduce without an explicit future decision:

- plugin ecosystem;
- user-provided arbitrary source providers;
- torrent downloads;
- encrypted archives;
- multipart archives;
- arbitrary source-defined scripts;
- mandatory online accounts;
- mandatory cloud services;
- telemetry;
- UI-owned installation logic.

---

# 45. Final target

Legio should behave as a single coherent product:

```text
Steam catalog
      +
Legio download source
      +
local game library
      +
persistent download/install system
      +
Windows/Linux launch engine
      +
Linux compatibility layer
      =
Legio Launcher
```

The result should feel closer to a modern full game launcher than to a download utility with a launch button.
