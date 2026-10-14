# Game library layout

New Store installations use a readable, portable name: `Games/My Game/Game.exe`.
Existing directories are preserved. A collision uses `My Game (2)`, then the next
available number, including on case-sensitive filesystems. Download and game IDs
remain stable internally and are independent of directory names.

`library.ldf` uses TOML with an explicit format version:

```toml
format = 1

[[games]]
id = "54378de9-bed5-49a2-8793-0f91f7277e03"
name = "My Game"
folder = "My Game"
executable = "Game.exe"
steam_app_id = 123
source_id = "personal-source"
entry_id = "stable-variant-id"
version = "1.2.0"
```

Each installation library records its games with paths relative to that library.
The default library also catalogs manual and transferred installations outside
that library, using absolute folder paths. Executables are relative to their game
folder. Optional source and release fields are omitted when unavailable. The file
is written through a flushed temporary file and replaced atomically. Invalid or
unsupported manifests are reported and preserved instead of overwritten.

The runtime database continues to hold launch preferences, playtime, queue state
and recovery journals. `library.ldf` records the installation mapping and release
metadata; it is not a replacement for the application database or an editable
launch configuration. Import, executable selection, installation, update,
transfer, removal and startup keep the registry in sync.

## Detecting game folders

A normal executable uses its parent directory. An executable named
`*-Win64-Shipping.exe` immediately under `X/Y/Binaries/Win64` uses `X` as the game
folder, preserving sibling Engine files. Filename and Unreal directory matching
are case-insensitive, and the project directory and executable names may differ.
Archive wrappers above the detected folder are not installed.

Transfers ask only for a destination library. The backend detects the source,
chooses a safe readable destination name, checks for shared game files and
prevents overlap. An active launcher session or externally running process blocks
the operation. Running-state checks also protect publication and source cleanup.

Interrupted copies retain their ownership evidence and database recovery intent.
Successful installations remove temporary copy/install markers from the game
folder. Startup also removes matching completed-install markers left by older
versions without reorganizing existing UUID directories. Existing UUID installs
retain their update layout until they are transferred into the new layout.
