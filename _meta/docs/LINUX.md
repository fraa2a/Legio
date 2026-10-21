# Linux runners and performance controls

## What to install

| Game launch method | Requirements |
| --- | --- |
| Native Linux | No compatibility runner |
| Wine | Wine |
| Manual Proton / GE-Proton | A compatible runner and [umu-launcher](https://github.com/Open-Wine-Components/umu-launcher) |
| Proton with Online Fix | Runner and its required Steam Linux Runtime installed in Steam; no umu |
| Steam-managed game | Steam handles launch |

**AppImage, .deb and .rpm:** install umu separately. `umu-run` must be in PATH or `~/.local/bin`.

**AUR (`legio-launcher-bin`):** umu is a package dependency. Enable Arch's `multilib` repository.

umu downloads its required Steam Linux Runtime, but not Proton itself. For Online Fix, install the runtime requested by the runner. Manual games need Steam only for Steam integration or Online Fix.

Existing prefixes and saves stay in place. Only one game can use a shared prefix at a time.

## Optional controls

Install [GameMode](https://github.com/FeralInteractive/gamemode) or [gamescope](https://github.com/ValveSoftware/gamescope), then reopen the game's compatibility panel. Their executables must be in PATH or `~/.local/bin`. Both are off by default and apply to manually imported games.

- **GameMode:** requires its user service and preload libraries; Legio does not configure them.
- **gamescope:** offers an internal resolution and optional FPS cap. Lower resolution affects quality; the extra compositor can affect latency.

These controls and Wine Wayland are independent. Test one change at a time; an FPS gain is not guaranteed.

<details>
<summary>Launch order and environment</summary>

Commands use `gamescope [options] -- gamemoderun umu-run game.exe [arguments]`, omitting disabled wrappers. Online Fix replaces umu with `runtime/run -- proton run`; Wine uses its own executable. Steam-managed games keep the Steam launch path.

Runner-default graphics and Wayland options clear inherited overrides for those settings. Explicit shader-cache overrides are preserved; [vkd3d](https://github.com/HansKristian-Work/vkd3d-proton) uses a per-game cache directory. Set umu's `GAMEID` and `STORE` explicitly when needed; Legio does not derive an umu ID from the Steam App ID.

</details>

For transparency and Wayland/XWayland problems, see [Linux rendering diagnostics](LINUX-RENDERING.md).
