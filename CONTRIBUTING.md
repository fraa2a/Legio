# Contributing to Legio

## Get started

- Follow [Development setup](_meta/docs/DEVELOPMENT.md).
- Check [Known issues](_meta/docs/KNOWN-ISSUES.md) before reporting a bug. Include reproduction steps and platform details.
- Discuss substantial changes in an issue first.
- Report vulnerabilities privately through [SECURITY.md](SECURITY.md).

## License

Legio uses source-available SA-NC-PD 1.0, not an open-source license. Read [LICENSE](LICENSE), especially section 5: contributions are permitted, but general distribution of modified versions is restricted. Submitting a contribution grants the rights described there. Only submit material you have the right to contribute.

## Pull requests

1. Create a short-lived `feat/*`, `fix/*`, `refactor/*` or `chore/*` branch. Keep `main` releasable.
2. Keep one coherent change per PR and add regression tests for fixes.
3. Use Conventional Commits, such as `feat:`, `fix:`, `docs:` or `test:`.
4. Run the relevant [checks and tests](_meta/docs/TESTING.md). The `frontend`, `rust` and `tauri-smoke` CI jobs must pass.
5. Update from `main` when needed, squash merge, then delete the branch.

## Code and documentation

- Rust owns business logic and native integration. Keep Tauri commands thin and frontend IPC in service modules.
- Keep Svelte components focused on UI; reuse existing primitives.
- Prefer focused changes over speculative abstractions. Handle errors and remove dead code.
- Keep current documentation in `_meta/docs` and images in `_meta/screenshots`.
- Put review results in PRs and release notes in GitHub Releases. Update known issues when taking ownership or resolving them.

## Versioning

`src-tauri/Cargo.toml` owns the version. Do not bump it during normal work: an increase requests a release. Follow Semantic Versioning; never reuse or decrease released versions. Create tags manually only for release recovery. See [Releases and updates](_meta/docs/UPDATING.md).
