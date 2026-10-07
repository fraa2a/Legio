<div align="center">
  <img src="src-tauri/icons/icon.png" alt="Legio" width="96" />
  <h1>Legio</h1>
  <p>Your game library, with launch settings tailored to each title.</p>
  <p><a href="https://github.com/fraa2a/Legio/releases">Download Legio</a></p>
</div>

Legio is a desktop game launcher for Windows and Linux. Import games from Steam or add them manually, configure how each game launches, and keep your library and playtime in one place.

![Legio Launcher home screen with recently played games and play activity](_meta/screenshots/legio-launcher.png)

## Features

- **A Steam account for each game.** If you use multiple accounts, assign the right one to each title. Legio asks before restarting Steam to switch accounts when needed.
- **Per-game launch settings.** Run native Linux games or Windows games through Proton, GE-Proton, or Wine. Configure prefixes, launch arguments, environment variables, and DLL overrides on Linux, or native launch options on Windows.
- **Flexible game import.** Detect your Steam library or add games and executables manually.
- **Themes you can make your own.** Choose from built-in palettes, edit colors, set image or animated backgrounds, and save your own themes. Import and export themes as JSON.
- **Hyprland transparency.** Enable a transparent Legio window on Hyprland and adjust the surface opacity. Background blur is controlled by your Hyprland rules.
- **Playtime at a glance.** See recently played games, tracked playtime, and a calendar of your activity.

## Online Fix

Legio includes an Online Fix launch option for compatible games. It is a launch configuration feature: Legio does not provide or distribute game files or Online Fix files.

Legio does not support or promote piracy. Use this option only with a legitimately purchased copy of the game and for offline play, in accordance with the game's license and applicable laws. Users are responsible for their use of this feature; Legio accepts no responsibility for misuse.

## Download

Get the latest version from [Releases](https://github.com/fraa2a/Legio/releases). Packages are available for Windows and Linux. To run Windows games on Linux, install a compatible runner such as Proton or Wine first.

Steam must be installed for Steam integration and for games that require it.

Manual Proton and GE-Proton games use [umu-launcher](https://github.com/Open-Wine-Components/umu-launcher).
Install `umu-run` on your PATH or in `~/.local/bin`; umu manages the required runtime.
Steam-managed games continue to launch through Steam. Manual games only need Steam
when Steam integration or Online Fix is enabled. Existing Wine and Proton prefixes
are reused without moving save files. A shared prefix can run one game at a time.

The runner's graphics and Wayland defaults clear inherited overrides for those two
options. Explicit environment overrides for shader caching are preserved. vkd3d
shader caches use a separate per-game cache directory. Game-specific umu fixes can
be selected with the documented `GAMEID` and `STORE` environment variables; Legio
does not infer an umu ID from a Steam App ID.

## Documentation

- [Source schemas and download flow](_meta/docs/SCHEMA.md)
- [Releases and updates](_meta/docs/UPDATING.md)

## License

See [LICENSE](LICENSE) for the terms of use and distribution.

## Discord Rich Presence

Enable Discord Rich Presence in **Settings > General** to share the running game's
name and session timer. When no game is running, Discord shows Legio's library
activity. If multiple games are running, the most recently started game is shown.
The feature is disabled by default and works through local Discord desktop IPC
on Linux and Windows; it does not require a bot token or a Discord login in Legio.

Legio's Discord Application ID (`1557120430475575366`) is configured by default.
Enable the toggle, keep Discord desktop running, and enable activity sharing in
Discord's privacy/activity settings. No setup in the Developer Portal, client
secret or bot token is needed. The Application ID field is an optional override
for users who want to use their own Discord application. Empty IDs saved by earlier
builds fall back to Legio's official application.

Legio retries if Discord starts later or restarts. Disabling the toggle clears
Legio's activity and disconnects; exiting Legio closes its IPC connection. Closing
the window to the tray keeps Rich Presence active. Only the game name and session
start time are published, never executable paths, Steam accounts or launch options.

### Work during a game session

Backgrounds, media and CSS animations pause when Legio loses focus, is minimized or is hidden in the tray, and during a tracked game session. Recent artwork prefetch and scheduled Steam scans wait until the session ends. General settings offer an optional switch to defer automatic download extraction until all tracked games are idle; existing extractions finish safely and network downloads continue. Explicit installation/finalization actions remain under user control. Game activity arrives through native events with a periodic recovery sync. Shortcut preparation runs outside the UI thread.

### Optional Linux performance controls

The compatibility panel of each manually imported game offers GameMode and gamescope. They are disabled by default and require the corresponding executable in PATH or ~/.local/bin. Install the distribution packages and reopen the panel to refresh availability. GameMode also needs a working user service and its preload libraries; Legio does not change its system configuration.

With gamescope enabled, choose a native or explicit internal resolution and an optional frame cap. Commands use the structured order `gamescope [numeric options] -- gamemoderun umu-run game.exe [game arguments]`; disabled wrappers are omitted and Wine replaces umu-run for Wine runners. Steam-managed games keep their existing client launch path. Resolution and FPS are validated before saving and launching. A lower internal resolution changes visual quality, and the additional compositor can change latency. GameMode, gamescope and native Wine Wayland are independent controls. Compare each change against the default for the same game; no FPS gain is guaranteed.

References: [umu-launcher](https://github.com/Open-Wine-Components/umu-launcher), [vkd3d-proton shader cache](https://github.com/HansKristian-Work/vkd3d-proton), [GameMode](https://github.com/FeralInteractive/gamemode) and [gamescope](https://github.com/ValveSoftware/gamescope).

Transfer recovery runs on a blocking worker during startup. Application hydration and desktop shortcuts wait asynchronously for recovery. Launching, removal, transfers, import, finalization and download mutations cannot race incomplete recovery, and a recovery failure remains visible in the launch error banner. Download recovery retains its existing dedicated worker.

UI refresh timers for playtime, monthly activity and news stop while the window is inactive and refresh on focus restore. Download progress polling and the event-backed launch recovery poll also stop while hidden; downloads and native lifecycle monitoring keep running. The launch polling fallback remains active if native event subscription fails. Steam artwork refresh timers wait until the window is active.
