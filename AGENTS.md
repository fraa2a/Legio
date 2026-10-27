# AGENTS.md

## Project

- Rust owns business logic, filesystem, processes, downloads, installation and system integration. Svelte owns presentation, interaction and frontend state.
- Keep Tauri commands thin; put logic in focused Rust modules or services.
- Reuse existing conventions and primitives. Do not add abstractions, layers, dependencies or options without a current need.
- Make the smallest coherent change. Do not mix refactors with feature or bug-fix work.
- Never use em dashes or en dashes, in code, comments, docs, UI copy or commit messages. Use standard punctuation or hyphens.

## Commands

- Frontend: `npm run check`, `npm run lint`, and the relevant `npm run test:*` script.
- Rust: `cargo test` in `src-tauri`.
- Run the checks relevant to your change. Do not call work complete while they fail.

## Git

- `main` must stay buildable and releasable. Work on short-lived `feat/`, `fix/`, `refactor/`, `chore/` or `docs/` branches.
- One branch and pull request per coherent change. Keep them small.
- Write branch names, commit messages, PR titles and PR descriptions in English.
- Use Conventional Commits: `feat:`, `fix:`, `refactor:`, `chore:`, `docs:`, `test:`, `build:`, `perf:`.
- Merge only after required checks pass. Squash merge, then delete the branch.

## Versioning

- `src-tauri/Cargo.toml` is the single source of truth for the version.
- Do not change the version during normal work; a version change requests a release.
- Never reuse, decrease or manually tag a released version, except to recover a failed release.

## Rust

- Use `Result` and `Option` and return meaningful errors. Avoid `unwrap()` and `expect()` in production paths unless failure is impossible.
- Prefer strong types and enums that make invalid states unrepresentable. Minimize shared mutable state and public APIs.
- Keep async boundaries explicit. Never run expensive synchronous work on an async executor.
- Avoid `unsafe`. If unavoidable, keep its scope minimal and document the safety constraint.

## Svelte and TypeScript

- Keep business logic out of `.svelte` files; move reusable logic and state into modules.
- Keep Tauri `invoke()` calls in dedicated frontend service modules, not in components.
- Derive state instead of duplicating it. Avoid unnecessary reactive state.
- Reusable UI primitives go in `src/lib/components/ui`; feature components stay with their feature.
- Use Tailwind and existing design tokens before arbitrary values. Keep equivalent controls consistent. Respect reduced-motion preferences.

## Naming and Comments

- Name functions for the operation they perform. Reserve `is_*`, `has_*` and `can_*` for boolean-returning functions.
- Keep test names short. Put non-obvious fixture details in a local comment, not in the name.
- Comments are short factual notes on non-obvious constraints. Do not narrate code. Update or remove comments when the related code changes.
- TODOs must describe a concrete remaining task.

## Errors, Logging and Security

- Never silently ignore errors. Show useful user-facing messages and keep technical details for debugging.
- Log state changes and failures, not routine calls. Never log secrets, tokens or personal data.
- Validate input at boundaries. Treat paths, downloads, remote manifests and process output as untrusted.
- Verify downloads with hashes or signatures when available. Grant only the Tauri capabilities and permissions required.
- Never commit secrets. Never build shell commands from untrusted strings; pass structured arguments.

## Dependencies and Structure

- Add a dependency only when it gives clear value and the project does not already solve the problem. Remove unused ones.
- Put code where its responsibility belongs. Avoid catch-all files such as `utils.ts` or `helpers.rs`.
- Do not commit generated files unless explicitly required.

## Changes and Tests

- Understand existing code before changing it. Preserve current behavior unless the task changes it.
- Leave no dead code, commented-out code, debug output or unused dependencies.
