# Contributing to Legio

## Branches

Keep `main` buildable and releasable. Use a short-lived `feat/*`, `fix/*`, `refactor/*`, or `chore/*` branch for normal development. Keep each branch and pull request focused on one coherent change.

## Commits and pull requests

Use Conventional Commit prefixes such as `feat:`, `fix:`, `refactor:`, `chore:`, `docs:`, `test:`, `build:`, and `perf:`. Required checks must pass before merge. Update from `main` when necessary, squash merge, and delete the branch after merge.

## Required checks

Run the relevant local commands before opening a pull request:

```sh
corepack pnpm install --frozen-lockfile
corepack pnpm check
corepack pnpm lint
corepack pnpm build
cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --all-features --locked -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml --all-features --locked
corepack pnpm tauri build --debug --no-bundle
```

Pull requests are expected to pass the `frontend`, `rust`, and `tauri-smoke` CI jobs.

## Version ownership

`src-tauri/Cargo.toml` is the single source of truth for the application version. Do not bump the version during normal work. A version increase explicitly requests a release and must follow Semantic Versioning. Never decrease or reuse a released version, and do not create release tags manually outside release recovery.

## Engineering expectations

Follow `AGENTS.md` and the canonical `PLAN.md`. Keep Tauri commands thin, put application logic in focused Rust modules when real behavior requires them, and keep business logic out of Svelte components. Make the smallest coherent change, reuse existing conventions, and do not leave dead code or ignored failures.
