# Application smoke test

`smoke.mjs` drives the built Legio binary through [tauri-driver](https://v2.tauri.app/develop/tests/webdriver/) (W3C WebDriver). It uses a small built-in WebDriver client, so it needs no npm dependency. One run:

1. Starts the application with a fresh, temporary profile and waits for the application shell.
2. Completes onboarding with the default choices.
3. Adds a game through **Add game** using a fixture executable created in a temporary folder.
4. Opens the game page, Home and Store, and every settings category, failing on any error banner.
5. Measures the performance metrics below, hides the window to the tray, shows it again through a second launch and quits through the title bar.

Selectors use English accessible names, so the harness sets `LANGUAGE=en`. The native file picker cannot be driven through WebDriver; the harness answers only its `plugin:dialog|open` IPC request with the fixture path. Everything else runs through the real UI and backend.

## Isolation

On Linux the application gets temporary `HOME` and `XDG_*` directories and a private D-Bus session bus without service activation, so it never reads or writes the real profile and never reaches a Legio instance that is already running. The temporary profile is deleted at the end.

On Windows, application data paths come from known folders that ignore environment overrides, and the single-instance check is machine-wide. Outside CI (`CI=true`) the harness refuses to run when a Legio profile exists for the current user, and it removes the profile folders it created when it finishes. Use a disposable account or VM, and do not run Legio during the test.

## Running locally

Build the binary first: `corepack pnpm tauri build --debug --no-bundle`.

Linux needs `webkit2gtk-driver` (for `WebKitWebDriver`), `xvfb` and `dbus-daemon`:

```sh
sudo apt-get install webkit2gtk-driver xvfb
cargo install tauri-driver --version 2.1.0 --locked
xvfb-run -a -s "-screen 0 1920x1080x24" corepack pnpm smoke:app --output /tmp/legio-smoke
```

The default xvfb-run screen is too small for the 1280 x 720 window. Without a GPU, WebKitGTK falls back to software rendering and may print EGL warnings; if rendering fails, retry with `WEBKIT_DISABLE_DMABUF_RENDERER=1` (see [Linux rendering diagnostics](../../_meta/docs/LINUX-RENDERING.md)). The harness sets `GTK_THEME=Adwaita:dark` unless it is already set, so the System palette and screenshots are dark.

Windows needs `msedgedriver.exe` matching the installed WebView2 runtime version:

```sh
cargo install tauri-driver --version 2.1.0 --locked
corepack pnpm smoke:app --application src-tauri/target/debug/legio.exe --native-driver C:\path\to\msedgedriver.exe
```

Options: `--application` (default: the debug binary in `CARGO_TARGET_DIR` or `src-tauri/target`), `--output` (default: a new temporary folder), `--tauri-driver`, `--native-driver`, `--budget`, `--idle-seconds` (default 15) and `--cpu-seconds` (default 20).

## Output

The output folder contains `results.json` (steps, timings and metrics), `driver.log` (tauri-driver, WebDriver and application output), `console.log` (WebView console messages and uncaught errors captured after startup) and `screenshots/` at 1280 x 720: `onboarding`, `library`, `game`, `home`, `store`, `settings` and one `settings-NN` per category. A failure also writes `failure.png`. CI uploads this folder as the `legio-smoke-*` artifact on every run.

## Performance budget

| Metric | Measurement | Platforms |
| --- | --- | --- |
| `startupMs` | From the WebDriver session request, which launches the process, until the application shell is in the DOM on a fresh profile | Linux, Windows |
| `idleMemoryMiB` | Proportional set size (PSS) of the application and its WebKit processes after `--idle-seconds` on Home | Linux |
| `hiddenCpuPercent` | CPU time of the same processes over `--cpu-seconds` while hidden in the tray, in percent of one core | Linux |

Windows memory and CPU are not measured, because doing so without new native tools is not reliable. CI measures the debug build that the native job already produces; release runs reuse the production build and skip the smoke test. Debug numbers are not representative of releases, so the budget only catches large regressions.

[`performance-budget.json`](performance-budget.json) holds a `max` per platform and metric. A run fails only when a measurement exceeds its `max`; results and the budget are written to the job summary. The initial limits are provisional: derived from local debug runs (about 1.2 s start, 1000 MiB PSS, under 1% hidden CPU) with wide headroom for slower CI runners. To update the budget, take the values from several recent `results.json` artifacts of the same platform, set `max` well above the worst of them (about twice the typical value), set `provisional` to `false` once CI data backs it, and explain the change in the pull request.

## Documentation screenshots

`update-screenshots.mjs` copies the Home, Library, game page, Store and Settings captures into [`_meta/screenshots/app`](../../_meta/screenshots/app). Download a `legio-smoke-Linux-*` artifact or run the smoke test locally, then:

```sh
node tools/app-smoke/update-screenshots.mjs /tmp/legio-smoke
```

Check the images before committing them.
