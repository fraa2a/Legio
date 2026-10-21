# Linux rendering diagnostics

WebKitGTK 2.54 and newer use Skia composition by default in Legio, including Hyprland. Legio no longer changes the process environment to select TextureMapper. Transparent native windows, blur, artwork, WebGL backgrounds and foreground fades remain supported.

The legacy GTK full-window damage handler is installed only for transparent windows using a pre-2.54 runtime or an explicit TextureMapper preference. The normal 2.54+ Skia path leaves GTK invalidation and focus redraws to the renderer.

## Native diagnostics

Enable application logging in Settings, fully exit Legio and start it again. Application logging and network logging remain independently disabled by default. `app.txt` contains a `linux_renderer` entry with:

- Loaded WebKitGTK version, rather than the build-time pkg-config version.
- Actual GDK display type: `GdkWaylandDisplay` or `GdkX11Display`.
- Read-back Skia and damage-composition preferences, explicit overrides, native alpha and legacy GTK damage handling.
- Hardware acceleration policy and explicit DMA-BUF/shared-memory switches.

Preferences and acceleration policy do not prove which GPU executed a frame. WebKit's internal fallback or a driver issue can still affect presentation. Do not describe this record as GPU telemetry. Configuration failures produce `linux_rendering_configuration` records; a terminated WebProcess produces a durable `linux_web_process_terminated` record. These observers do not guarantee a log for every host/GPU failure.

## Controlled comparisons

Use the same binary, game, cached artwork, settings, display scaling and driver for each comparison. Fully quit between runs, including the tray instance, because an existing instance retains its original renderer and environment. The normal run needs no overrides.

For Legio, compare explicit native Wayland (`GDK_BACKEND=wayland`) with XWayland (`GDK_BACKEND=x11`) while using the same Skia default. Explicit `WEBKIT_USE_SKIA_FOR_COMPOSITION=0` remains an engine-level compatibility comparison where TextureMapper is still available. Clear that switch for normal Skia use.

Do not use a live feature setter to test damage composition: WebKit 2.54 latches compositor settings when it constructs the renderer. A getter can report an updated preference without changing the already-created compositor. Legio's diagnostics therefore read settings without writing them. `LEGIO_WEBKIT_DAMAGE_COMPOSITING` is unsupported and produces a warning if supplied.

For damage, transport and native alpha comparisons, use the [standalone probe](../../tools/linux-rendering/README.md) in `tools/linux-rendering/probe.c`. It constructs an ephemeral WebView with preferences set before WebPage creation, uses local synthetic artwork and blur, and does not load a Legio profile. Build with WebKitGTK 2.54+ development headers:

```sh
cc -std=c11 -Wall -Wextra -Werror tools/linux-rendering/probe.c -o /tmp/legio-rendering-probe $(pkg-config --cflags --libs webkit2gtk-4.1)
```

Clear inherited `WEBKIT_USE_SKIA_FOR_COMPOSITION`, `WEBKIT_DISABLE_DMABUF_RENDERER`, `WEBKIT_DMABUF_RENDERER_FORCE_SHM`, `WEBKIT_SKIA_ENABLE_CPU_RENDERING`, `WEBKIT_SHOW_DAMAGE` and `LEGIO_WEBKIT_DAMAGE_COMPOSITING` between comparisons, unless the row explicitly sets one. The probe rejects conflicting composition overrides. The damage overlay can force full repainting and mask a bug. Shared-memory transport is a diagnostic comparison with a potential performance cost.

| Probe run | Environment and arguments | Purpose |
| --- | --- | --- |
| Native Wayland | `GDK_BACKEND=wayland /tmp/legio-rendering-probe` | Skia with native damage tracking |
| XWayland | `GDK_BACKEND=x11 /tmp/legio-rendering-probe` | Change only the display backend |
| Full composition | `GDK_BACKEND=wayland /tmp/legio-rendering-probe --full-composition` | Keep Skia, disable damage-limited composition before renderer creation |
| Shared memory | `GDK_BACKEND=wayland WEBKIT_DMABUF_RENDERER_FORCE_SHM=1 /tmp/legio-rendering-probe` | Compare buffer transport independently |
| Opaque presentation | `GDK_BACKEND=wayland /tmp/legio-rendering-probe --opaque` | Compare opaque versus transparent content presentation |
| Explicit compatibility | `GDK_BACKEND=wayland /tmp/legio-rendering-probe --texture-mapper` | Compare TextureMapper where the engine still provides it |

Append `--check` for a nonvisual initialization check that reads preferences and constructs the ephemeral view without opening a window. It does not validate pixels or GPU execution. The synthetic probe is a reduction aid; a passing probe cannot prove that Legio's complete game page is unaffected.

Open a game page containing hero artwork and blurred panels. Repeat window focus changes and workspace changes at least 20 times, then minimize/restore, resize, open/close fullscreen artwork and navigate through Library, Store and Settings. Repeat with Dither, another background and no background. Check that no surface disappears, text remains sharp, the sidebar stays visible and idle GPU use does not grow unexpectedly. Record the failing comparison and the corresponding `linux_renderer` entry.

No additional startup navigation or process restart is added to Legio. Its normal Skia selection comes from WebKit's initial engine configuration, before Tauri setup. Explicit engine environment overrides are reported separately from the read-back preferences, because an environment override can affect the WebProcess without changing the UI-process settings object.

## Runtime distribution boundary

The current deb/rpm/AppImage release pipeline builds on Ubuntu 22.04 and does not establish a controlled WebKitGTK 2.54 runtime. Upgrading application code or shipping an ordinary package does not upgrade a user's system engine. Legacy engines continue to start and are identified in the log.

A controlled runtime must pin the engine and required libraries, include its matching WebProcess/network helpers, and be tested as an assembled package. It must use a maintained 2.54+ version with an update/backport policy, verify the actual loaded version and pass the comparisons above on affected hardware. Do not mix an arbitrary WebKit library with host helpers or describe an LD_LIBRARY_PATH override alone as isolation. A package that claims a controlled runtime must satisfy these requirements independently of application code changes.
