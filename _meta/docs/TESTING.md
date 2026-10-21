# Testing

Complete [Development setup](DEVELOPMENT.md), then run commands from the repository root.

## Automated checks

```sh
corepack pnpm install --frozen-lockfile
corepack pnpm check
corepack pnpm lint
corepack pnpm build
cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --all-features --locked -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml --all-features --locked
python -B -m unittest discover -s scripts
corepack pnpm tauri build --debug --no-bundle
```

Also run the relevant `test:*` suites in [package.json](../../package.json), for example:

```sh
corepack pnpm test:regressions
corepack pnpm test:review
corepack pnpm test:feedback
corepack pnpm test:sources
```

The [CI workflow](../../.github/workflows/ci.yml) lists required suites. [Scripts](../../scripts/README.md) describes fixtures and test support. JSDOM and mock graphics tests do not validate native rendering.

Native tests need platform dependencies, localhost sockets and access to their child processes. Run relevant ignored or interactive tests explicitly; a passing subset does not replace required CI checks.

## Desktop checklist

Test development and installed builds. Back up production data before testing installers.

- **Library:** empty/populated states, restart persistence, Steam detection, launch/stop and playtime.
- **Downloads:** pause, resume, cancel, extract and finalize using an authorized source.
- **Store:** sorting, filters, release selection, source refresh and offline cache. Rapid changes must not show stale results.
- **UI:** both themes/languages, keyboard focus, retries, reduced motion, minimum window size, scaling and both sidebar states.
- **Artwork:** cold/warm/expired caches, offline refresh, quick A/B/A navigation and cancellation. Failed refreshes must retain displayed content.
- **Rendering:** animated/frozen/disabled backgrounds, resize, minimize/restore and GPU context recovery. See [Linux diagnostics](LINUX-RENDERING.md).
- **Logging:** independent application/network toggles, off by default, and redaction. See [Troubleshooting](TROUBLESHOOTING.md).

Record reproduction details and results in the issue or PR, including build, OS, package, runtime, driver, display backend and scaling.

## PR test packages

CI publishes Linux and Windows packages for pull requests. Check `build-info.txt` against the PR revision and verify `SHA256SUMS.txt`. Windows symbols are a separate artifact.

**These are unsigned-updater test packages, not releases. Installers use production data; back it up first.** Development runs use a separate profile.

## Performance

Keep build configuration, games, settings, scaling and network conditions constant. Compare cold/warm caches and logging off/on. Measure:

- Time to visible covers and selected-game artwork.
- Idle CPU/memory with the window visible and hidden.
- Same-volume and cross-volume transfer time.

Separate queue wait, transfer, conversion and publication times. Queue tests alone do not prove a speedup.
