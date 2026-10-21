# Linux rendering probe

`probe.c` is an optional standalone GTK/WebKitGTK diagnostic. It renders a synthetic page with artwork-like surfaces, blur, transitions and a modal dialog, allowing backend comparisons without opening Legio or loading a user profile. It is not compiled into the launcher and is not a runtime dependency.

## Requirements and build

Use Linux with a graphical GTK session, a C compiler, pkg-config, GTK 3 development files and WebKitGTK 4.1 development files exposing the 2.54+ feature API. Both the build headers and loaded WebKitGTK runtime must be suitable. Run from the repository root:

```sh
cc -std=c11 -Wall -Wextra -Werror tools/linux-rendering/probe.c \
  -o /tmp/legio-rendering-probe $(pkg-config --cflags --libs webkit2gtk-4.1)
/tmp/legio-rendering-probe --help
/tmp/legio-rendering-probe
```

The output binary is outside the checkout and should not be committed.

## Options

| Option | Effect |
| --- | --- |
| No options | Transparent window with Skia and damage-limited composition |
| `--full-composition` | Disable damage-limited composition before WebView creation |
| `--texture-mapper` | Request TextureMapper where the engine supports it |
| `--opaque` | Use opaque native presentation for comparison |
| `--check` | Initialize GTK and construct the ephemeral WebView without opening a window |
| `--help` | Print supported options |

`--check` still requires display access. A successful initialization does not establish correct pixels or GPU behavior. Conflicting `WEBKIT_USE_SKIA_FOR_COMPOSITION` settings are rejected.

Follow [Linux rendering diagnostics](../../_meta/docs/LINUX-RENDERING.md) for controlled Wayland/XWayland, transport and alpha comparisons. Record failures in an issue with runtime, backend, driver and exact command, rather than committing machine-specific results here.
