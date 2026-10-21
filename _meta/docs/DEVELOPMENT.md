# Development

## Prerequisites

- Git.
- Node.js 22 and Corepack. `package.json` pins the pnpm version.
- Rust through rustup. `rust-toolchain.toml` pins the toolchain and includes rustfmt and Clippy.
- The platform dependencies below. See the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) for other distributions and installation details.

### Linux

On Ubuntu or Debian, install:

```sh
sudo apt-get update
sudo apt-get install -y build-essential curl file pkg-config \
  libayatana-appindicator3-dev libarchive-dev librsvg2-dev libssl-dev \
  libwebkit2gtk-4.1-dev libxdo-dev wget
```

Interactive development and rendering checks require a graphical session.

### Windows

Install Microsoft C++ Build Tools with the Desktop development with C++ workload, the Windows SDK, and the WebView2 runtime. Native archive support uses vcpkg with the `x64-windows-static-md` triplet:

```powershell
git clone https://github.com/microsoft/vcpkg.git C:\vcpkg
C:\vcpkg\bootstrap-vcpkg.bat
C:\vcpkg\vcpkg.exe install libarchive:x64-windows-static-md
$env:VCPKG_ROOT = "C:\vcpkg"
$env:VCPKGRS_TRIPLET = "x64-windows-static-md"
```

Set these variables in the shell used to build Legio. The [CI workflow](../../.github/workflows/ci.yml) defines the tested Linux and Windows build steps.

## Run locally

From the repository root:

```sh
corepack enable
corepack pnpm install --frozen-lockfile
corepack pnpm tauri dev
```

`corepack pnpm dev` runs only the frontend server. Native commands require the Tauri application. Follow [Testing](TESTING.md) before submitting changes.

## Application profiles

| Build | Profile |
| --- | --- |
| Development | `dev.fraa2a.legio.dev` |
| Installed | `dev.fraa2a.legio` |

Development starts with its own empty library and preserves it across runs. It does not change installed data. **Test installers use production data: back it up first.**

Linux development database: `$XDG_DATA_HOME/dev.fraa2a.legio.dev/legio.sqlite3`, defaulting to `~/.local/share` when `XDG_DATA_HOME` is unset. Settings shows the log directory.

## Repository layout

| Path | Responsibility |
| --- | --- |
| `src/` | Svelte presentation, frontend services and state |
| `src/lib/components/ui/` | Shared UI primitives |
| `src/lib/features/` | Feature-specific views and components |
| `src-tauri/src/` | Rust business logic, persistence, filesystem and process integration |
| `src-tauri/test-fixtures/` | Native test inputs, including intentionally unsafe archive cases |
| `scripts/` | Node regression tests, fixtures and Python release automation |
| `tools/` | Optional standalone developer diagnostics, not application runtime dependencies |
| `packaging/` | Distribution packaging files |
| `_meta/docs/` | Maintained technical documentation |
| `_meta/screenshots/` | Documentation images, separate from runtime assets |
| `_meta/licenses/` | Additional asset license records |
| `.github/` | CI, release workflows and contribution templates |

## Assets and local files

- Keep runtime fonts and licenses in `src/assets/fonts/`.
- Add third-party assets only when needed and redistributable.
- Keep design experiments outside the checkout or exclude them locally with `.git/info/exclude`.
- Do not commit credentials, dumps, game data or personal tool configuration.

See [Contributing](../../CONTRIBUTING.md) for code conventions, [Libraries](LIBRARY.md) for persistence and [Source schemas](SCHEMA.md) for data contracts.
