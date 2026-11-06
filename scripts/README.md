# Scripts and tests

Run from the repository root. [package.json](../package.json) defines `test:*` commands; [CI](../.github/workflows/ci.yml) selects required suites. See [Testing](../_meta/docs/TESTING.md) for native and desktop checks.

## Node

| Files | Purpose |
| --- | --- |
| `test_*.mjs` | Regression suites; use package commands for the required Node flags |
| `test_documentation.mjs` | Local Markdown links and image paths (`test:docs`) |
| `test_module_loader.mjs` | Svelte/TypeScript loading with mocks; not a standalone suite |
| `fixtures/gofile-wt.js` | Protocol fixture shared with Rust via `include_str!` |

Reuse the runner and name tests by behavior. Moving files requires updating package commands, fixture references and CI.

## Python automation

- `ci_plan.py`: selects documentation/code checks and detects release version changes.
- `test_ci_plan.py` and `test_packaging.py`: check CI selection, versions, wrapper checksums and bundled licenses.
- `release_manifest.py`: creates and validates updater metadata.
- `prepare_aur.py`: generates AUR package files.
Run all Python tests with `python -B -m unittest discover -s scripts`.

These use the standard library. Keep signing credentials private; see [Releases and updates](../_meta/docs/UPDATING.md).
