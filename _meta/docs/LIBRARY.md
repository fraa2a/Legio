# Game libraries

Each registered library contains one readable JSON `library.ldf`. Game folders use
portable names instead of UUID wrappers. IDs remain stable metadata. Existing
installations are adopted without moving files automatically.

```json
{
  "format": 1,
  "libraryId": "897b4f63-73b4-4b6d-a0b8-f7ba753be309",
  "games": [
    {
      "id": "54378de9-bed5-49a2-8793-0f91f7277e03",
      "name": "My Game",
      "installation": {
        "folder": "My Game",
        "executable": "bin/Game.exe",
        "archiveLayout": "direct"
      },
      "steamAppId": 123456,
      "update": {
        "sourceSha256": "8f756aa62590a9b82af74a5ddc9f3e24124f511fdf8cc585c358db0e5a80b7d8",
        "entryId": "my-game-windows",
        "installedVersion": "1.2.0"
      }
    }
  ]
}
```

The library's absolute directory is registered only in the local database.
Installation folders are direct children of that directory; executable paths are
relative to the game folder and use forward slashes on both platforms. Steam IDs
and update bindings are optional. Unregistered external manual games stay in the
local database without creating manifests in arbitrary parent directories.

## Reading, writing and recovery

The local database is the runtime index and holds preferences, playtime, download
history and operation journals. The manifest describes portable installations.
Download settings expose import of an existing library without moving its files.
Import validates IDs, paths, ordinary files and conflicting registrations before
indexing installations. On import, cached installations absent from that library are
detached without deleting their local preferences or session history. Imported source
trust is never implicitly granted.

SQLite triggers record pending manifest publication in the transaction that
changes an installation or its binding. Each publication reads a fresh snapshot
under an interprocess filesystem lock, flushes a same-directory temporary file,
then atomically replaces the manifest. A revision check prevents a concurrent
mutation from losing its pending status. A content fingerprint refuses to
silently overwrite a manifest changed by another profile.

Startup imports clean registered manifests and retries pending local publication
instead of importing stale records. A committed removal cannot be undone by an
old manifest after a restart. Publication failures keep pending state and expose
warnings in the libraries settings; they do not turn an already committed game
change into a failed command or block unrelated launch operations. Repair saves
the previous file under a unique backup name before rebuilding the catalog.

The previous TOML schema is migrated when read, preserving a unique legacy backup.
Old absolute references to external games are not copied into the new per-library
manifest. Unknown JSON fields and unsupported format versions are rejected and
preserved for explicit recovery.

## Update source identity

Only SHA-256 of the validated, serialized manifest URL is exported. The URL is
hashed when a source is added or migrated and stored in an indexed local column.
Updates keep using existing local source IDs. Imported hashes resolve against
that index; an unavailable source stays an unresolved binding until configured.
The content of the remote JSON does not affect its URL identity. Scheme, host and
default port follow URL parser normalization; path and query remain meaningful.

The local application still needs the URL to fetch updates. A hash does not encrypt
a predictable address or prove source trust. Hash matching never authorizes an
unverified variant, and adding a missing source clears inherited trust.

## Folders, transfers and update layout

Initial detection uses the executable's parent, except
`*-Win64-Shipping.exe` under `X/Y/Binaries/Win64`, which uses `X` to retain Engine
files. A valid recorded installation root takes precedence on later transfers or
executable changes. Every file below that root moves together, except temporary
Legio ownership markers. Executable names and relative paths are preserved.

Transfers ask for a destination library and preserve the source folder name,
using numeric suffixes for collisions. A legacy UUID folder uses the readable
game name instead. An active launcher session or externally detected game blocks
transfer preparation, publication and source cleanup.

A same-volume transfer renames the folder. A copy transfer keeps symbolic links
as links with the same target and never copies or walks through them. On Unix it
accepts only relative links whose target, resolved through the folder's own links
without following them outside it, stays inside the game folder; dangling links
that meet this rule are kept. Absolute or escaping links stop the copy before any
file is written, and other platforms refuse every link.

`archiveLayout` records `legacy` or `direct` explicitly. Transferring or renaming a
legacy installation does not change how future update archives are overlaid.
Filesystem/database journals remain necessary because copying games and updating
multiple manifests cannot be one ordinary file replacement transaction.

## Organization and views

Games can be marked as favorites, given a play status (backlog, playing, completed,
on hold, dropped) and added to any number of named collections. Versions of one
game share a library card, so the card and the game page change all of them at once
and read them together: a favorite or membership on any version counts, the status
comes from the shown version or else the first version that has one. Storage and
validation are described in [SCHEMA.md](SCHEMA.md#library-organization-in-the-local-database).

The star on a card toggles the favorite. The collection button on a card, in the
list and on the game page opens a dialog with the status and a switch per
collection, plus a field to create one. The library toolbar manages collections
(create, rename, reorder and delete with confirmation; games stay in the library).

The toolbar switches between three views:

- Grid: the cover grid, with a slider from 70% to 150% of the default tile width.
- List: a table with sortable name, playtime, last played and source columns, plus
  status. Clicking the current column flips the direction; a new column starts
  descending for numbers and dates and ascending for text.
- Shelves: one horizontal row per collection in collection order, then the games
  without a collection.

Every view applies the same sort (name, playtime, last played, recently added or
source) and filters (favorites only, status, collection or games without one).
Unknown dates sort last in both directions; games added before organization was
stored have no known addition date. View, sort, direction, cover size and filters
are saved in the application preferences under `libraryView`. A filter on a
deleted collection shows every game. Sorting, filtering and resizing glide the
grid cards near the viewport to their new places, without motion when reduced
motion is preferred. Disk usage is not part of the list yet.

## Launch profiles

A game can have up to 12 named launch profiles, such as "with mods", "vanilla" or
"windowed". A profile is a set of overrides of the game's launch settings; every
field it leaves out inherits the base settings, which stay the implicit
"Predefinito" profile. A game without profiles launches exactly as before and its
Play button is unchanged. Storage and validation are described in
[SCHEMA.md](SCHEMA.md#launch-profiles-in-the-local-database).

| Override | Applies to | Replaces |
| --- | --- | --- |
| Arguments | every game | Steam launch arguments, native launch arguments, or the arguments after the executable of a compatibility launch |
| Environment variables | manual games | The whole environment of the base settings (an empty map clears it) |
| Runner | Linux games run through a compatibility runner | The runner of the game or of the global default |
| GameMode and Gamescope | Linux games run through a compatibility runner | The whole Linux performance block |

Resolution happens in the Rust launch paths, once, before the mod loader arguments
are appended and before the final validation of arguments, environment and DLL
overrides. Overrides are checked with the same validators as the base settings when
saved and again at launch. A launch that names an unknown profile, or a profile of
another game, fails instead of falling back to the base settings. The backend
remembers the profile of the last launch that started; deleting that profile makes
the game return to the base settings, and removing a game removes its profiles.

Mod profiles are not part of a launch profile. A launch always uses the game's
active mod profile, because deploying another profile rewrites files in the game
folder and changes the active profile. Several executables per game do not exist
either, so there is no executable override.

Profiles are managed in the "Profili di avvio" section of the game settings, with
the same fields as the base settings and a switch per field that returns it to
inheriting. When a game has profiles, the Play button becomes a split control: the
main part launches the last used profile and the arrow opens a menu, expanding from
the button into one surface, to pick another one. Picking an entry launches it and
makes it the last used. Desktop shortcuts and `--launch-game=` always launch the
base settings, and library manifests do not carry profiles.

## Advanced playtime editing

The per-game danger section can set Legio's local playtime total in hours and
minutes, with explicit confirmation. A persisted adjustment preserves real session
history and calendar activity. Future sessions add time normally. Active sessions,
invalid totals and externally detected running games are rejected by the backend.
Steam account playtime is not modified.



## Import from other launchers

The initial setup (onboarding) offers games installed with Heroic, Lutris, itch and
Bottles, only when at least one is found. Detection is read-only: Legio never writes
to, locks or modifies another launcher's folders. Each launcher is read on its own, so
one unreadable launcher does not hide the others.

| Launcher | Platform | Read |
| --- | --- | --- |
| Heroic | Linux, Flatpak, Windows | In `~/.config/heroic`, the Flatpak config or `%APPDATA%/heroic`: `legendaryConfig/legendary/installed.json`, `gog_store/installed.json` (plus `goggame-<id>.info` in the game folder), `nile_config/nile/installed.json`, `sideload_apps/library.json`, `store_cache/*_library.json` for titles and `GamesConfig/<app>.json` for the Wine prefix and runner name |
| Lutris | Linux, Flatpak | `pga.db` (table `games`, runners `wine` and `linux`) and `games/<configpath>.yml` for `game.exe`, `game.prefix` and `wine.version` |
| itch | Linux, Flatpak, Windows | `db/butler.db` tables `caves`, `games` and `install_locations`; the executable comes from the `verdict` candidates only when unambiguous |
| Bottles | Linux, Flatpak | `bottles/<bottle>/bottle.yml`: `Runner` and `External_Programs`; the bottle folder is the Wine prefix |

SQLite databases are opened read-only and immutable. Files have size caps, entry
counts are bounded, symbolic links leaving the launcher folder are not followed and
every path is checked to exist before it is offered. The YAML files are read with a
tolerant scalar extractor that skips any value it cannot read with certainty.

Selected games go through the manual import path: the same executable validation and
duplicate detection as adding a game by hand. Games whose executable is already in
the library, or whose folder is already used by a library entry (including Steam
installs), are skipped. A game without a known executable is listed as not
importable, because the library cannot hold an entry without one. On Linux, the
detected Wine prefix of a Windows game becomes its prefix in the compatibility
settings; the runner name is shown but not applied. No database migration is needed.

## Disk usage

`folder_size.rs` walks an installation off the async executor. It never follows
links, counts only regular files, and marks the result partial when an entry cannot
be read. On Linux it sums allocated blocks (the stat call already reports them); on
Windows it sums apparent length. The walk checks a cancel flag per entry and pauses
briefly every 1024 entries and between games.

`disk_usage.rs` measures only when a view asks (`refresh_disk_usage`): library
settings, the "Cosa occupa spazio" dialog, or the game page for that single game.
Games are Legio installations plus Steam games whose install folder is already
recorded. Results are cached in `disk-usage.json` in the app cache folder, keyed by
game ID with folder path, folder modification time and measurement time. An entry
is dropped when the path or folder mtime differs or it is older than seven days,
and explicitly after install, update, transfer and uninstall. No database schema is
involved. `legio:disk-usage-changed` tells the UI to read the cache again.

`get_library_usage` reports per library the total and free bytes of its disk, the
bytes of its measured games and the remaining used space as "other". The figure is
flagged as an estimate while any game is unmeasured or partial. Uninstalling from
the usage dialog reuses the regular uninstall dialog and its backend path.
