# Phase 01: Foundation and application shell

## Status

Completed

## Canonical requirements

Owns `PLAN.md` sections 1, 2, 32, 38, 40, 41, 43, and 44.

## Objective

Establish a reproducible Tauri 2, Rust, Svelte 5, TypeScript, Vite, and Tailwind CSS 4 application, then prove the architecture with one real Rust-to-Svelte IPC flow.

## Scope

- Clean cutover from the placeholder root Rust binary to a root pnpm frontend and sole Rust crate under `src-tauri`.
- Exact Node, pnpm, and Rust toolchain declarations and lockfiles.
- Minimal native desktop shell with loading, success, error, and retry behavior.
- Mandatory frontend, Rust, Linux, and Windows build automation.
- Repository guidance for contributors and security reports.
- No SQLite, settings, library model, remote requests, fake data, inactive navigation, or speculative service layers.

## Milestones and principal tasks

### 01A: Clean foundation cutover

- [x] Remove the root Rust placeholder and create the sole `src-tauri` crate.
- [x] Pin Rust 1.98.1 with rustfmt and Clippy.
- [x] Configure Node 22.x, Corepack, pnpm 12.5.1, and frozen lockfiles.
- [x] Configure Svelte, TypeScript, Vite, Tailwind CSS 4, and ESLint.
- [x] Configure one minimally privileged Tauri window.

Verification: frozen pnpm install and locked Cargo fetch complete without lockfile changes; frontend and Rust configuration load successfully.

### 01B: Minimal desktop shell and real IPC

- [x] Implement `get_app_info` from Tauri package metadata and the Rust platform constant.
- [x] Keep `src/lib/services/app.ts` as the only frontend invoke wrapper.
- [x] Render loading, success, actionable failure, and real retry states.
- [x] Show the Cargo-derived name, version, and platform in the native shell.

Verification: the native application renders `Legio`, `0.1.0`, and `linux`; browser-only development reaches a backend-unavailable state and Retry invokes the backend again.

### 01C: Mandatory checks and build smoke

- [x] Pass frontend check, lint, and production build.
- [x] Pass Rust format, Clippy with denied warnings, and tests.
- [x] Pass a local Linux debug no-bundle Tauri build and native runtime smoke.
- [x] Configure mandatory Ubuntu and Windows Tauri smoke jobs.
- [x] Record a successful Windows CI run before completing the phase.

Verification: exact commands and observable results are recorded below. Windows proof must come from CI.

## Dependencies

None. This phase creates the prerequisites for every later phase.

## Completion criteria

- The root frontend and sole `src-tauri` crate are reproducible from committed lockfiles.
- No obsolete root Cargo package remains.
- The real native window proves Rust-to-Svelte IPC.
- Browser-only development fails usefully without fabricating app information.
- The `frontend`, `rust`, and `tauri-smoke` CI jobs are mandatory and ungated.
- Linux native runtime and Linux/Windows no-bundle builds have evidence.
- README, contribution guidance, security reporting, and roadmap links are current.

## Open technical decisions

- The visual mockup referenced by section 35 is absent. Final visual parity is a Phase 08 input and does not block this minimal shell.
- Branch protection must be configured in GitHub after the named checks exist.

## Update notes

- 2026-09-21: Phase opened. The repository started with a root `Hello, world!` Rust binary and conditional automation.

### Milestone evidence

#### 01A

- **Platform:** Linux x86_64.
- **Command or scenario:** `corepack pnpm install --frozen-lockfile` and `cargo fetch --manifest-path src-tauri/Cargo.toml --locked`, with SHA256 values captured before and after.
- **Observable result:** both commands completed successfully, pnpm used 12.5.1, and neither lockfile changed.
- **Remaining blockers:** None.

#### 01B

- **Platform:** Linux native Tauri window and a managed Chromium browser.
- **Command or scenario:** `corepack pnpm tauri dev`; inspect the native window; open `corepack pnpm dev` without the Tauri bridge; activate Retry while recording invoke calls.
- **Observable result:** the native Legio window rendered Cargo-derived values `Legio`, `0.1.0`, and `linux`. The browser rendered `Native backend unavailable`; Retry called `get_app_info` again and remained in the useful failure state.
- **Remaining blockers:** None.

#### 01C

- **Platform:** Linux x86_64 locally; Ubuntu and Windows GitHub-hosted runners.
- **Command or scenario:** local frontend and Rust quality commands; `corepack pnpm tauri build --debug --no-bundle`; GitHub Actions run `35624188349`.
- **Observable result:** Frontend passed in 19 seconds, Rust passed in 3 minutes 3 seconds, Linux Tauri smoke passed in 2 minutes 33 seconds, and Windows Tauri smoke passed in 3 minutes 16 seconds. The Windows build succeeded after adding the required `src-tauri/icons/icon.ico` resource.
- **Remaining blockers:** None.

- 2026-09-21: Phase completed after GitHub Actions run `35624188349` passed all required frontend, Rust, Linux, and Windows checks.
- 2026-09-21: A neutral transparent window icon was added only because Tauri code generation requires platform icon resources. Final branding remains a Phase 08 decision after the missing mockup is supplied.
