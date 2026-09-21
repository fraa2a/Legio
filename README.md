# Legio

Legio is a desktop game launcher and downloader for Windows and Linux. Rust owns application logic and system integration. Svelte owns presentation and interaction inside a Tauri 2 desktop shell.

The canonical product requirements are in [`PLAN.md`](PLAN.md). The maintained implementation roadmap is in [`docs/plans/README.md`](docs/plans/README.md).

## Current scope

Phase 01 establishes the repository toolchains and a minimal application shell. The current desktop screen performs one real Tauri IPC call and displays the application name, Cargo version, and platform supplied by Rust. Database, settings, game-library, Steam, download, installation, and launch behavior begin in later roadmap phases.

## Required toolchains

- Node.js 22.x
- Corepack with pnpm 12.5.1, as pinned by `package.json`
- Rust 1.98.1 with rustfmt and Clippy, as pinned by `rust-toolchain.toml`

Enable Corepack once if it is not already enabled:

```sh
corepack enable
```

## Native prerequisites

Tauri requires platform development tools in addition to the pinned language toolchains.

### Linux

Install a C compiler and linker, WebKitGTK 4.1 development headers, GTK-related development libraries, OpenSSL development headers, and the platform packages required by Tauri 2. On Debian or Ubuntu, the CI workflow in [`.github/workflows/ci.yml`](.github/workflows/ci.yml) is the maintained package reference.

### Windows

Install Microsoft C++ Build Tools with the Desktop development with C++ workload and the Microsoft Edge WebView2 runtime. Use a supported Windows SDK.

## Install dependencies

```sh
corepack pnpm install --frozen-lockfile
cargo fetch --manifest-path src-tauri/Cargo.toml --locked
```

## Development

Start the native application:

```sh
corepack pnpm tauri dev
```

Run the browser-only frontend when testing the explicit backend-unavailable state:

```sh
corepack pnpm dev
```

## Checks

```sh
corepack pnpm check
corepack pnpm lint
corepack pnpm build
cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --all-features --locked -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml --all-features --locked
corepack pnpm tauri build --debug --no-bundle
```

## Production build

```sh
corepack pnpm tauri build
```

The application version is owned only by `src-tauri/Cargo.toml`. Normal development must not change it.
