# AGENTS.md

## Project

- Rust owns business logic, filesystem, processes, downloads, installation and system integration; Svelte owns presentation, interaction and frontend state.
- Keep Tauri commands thin and put real logic in focused Rust modules/services.
- Follow YAGNI: prefer simple, explicit code; do not add abstractions, extension points, architectural layers or dependencies without a current concrete need.
- Reuse existing conventions and primitives; avoid duplication, but do not abstract merely to remove superficial repetition.
- Make the smallest coherent change required and do not mix unrelated refactors with feature or bug-fix work.
- Never use em dashes or en dashes in any context, including source text, comments, documentation, UI copy, commit messages and generated content; use standard punctuation or hyphens instead.

## Git

- `main` must remain buildable and releasable; normal development happens on short-lived `feat/*`, `fix/*`, `refactor/*` or `chore/*` branches.
- One branch and pull request should represent one coherent change; keep PRs focused and reasonably small.
- Merge only after required checks pass; update from `main` when needed, squash merge, then delete the branch.
- Use Conventional Commits such as `feat:`, `fix:`, `refactor:`, `chore:`, `docs:`, `test:`, `build:` and `perf:`; final history must use meaningful messages, while intermediate branch commits may be imperfect because PRs are squash-merged.

## Versioning

- `src-tauri/Cargo.toml` is the single source of truth for the application version.
- Follow Semantic Versioning `MAJOR.MINOR.PATCH`, with `-alpha.N`, `-beta.N` or `-rc.N` when needed.
- Do not bump the version during normal work; a version increase explicitly requests a release.
- Merging a PR with a higher valid version triggers the release pipeline, which creates the matching `vMAJOR.MINOR.PATCH` tag.
- Never decrease, reuse or manually tag a released version except when recovering from a failed release process.

## Rust

- Prefer idiomatic ownership and borrowing; never clone merely to silence the borrow checker without understanding the ownership problem.
- Use `Result` and `Option` idiomatically; return meaningful errors instead of panicking, and avoid `unwrap()` or `expect()` in production paths unless failure is impossible by construction.
- Prefer strong types and enums when they prevent invalid states; minimize global/shared mutable state and unnecessary public APIs.
- Keep functions and modules focused; keep async boundaries explicit and never block an async executor with expensive synchronous work.
- Validate external input at boundaries; treat paths, downloaded data and process output as untrusted.
- Avoid `unsafe`; when unavoidable, keep its scope minimal and document the concrete safety constraint.

## Svelte and TypeScript

- Keep business logic out of `.svelte` files; components should focus on presentation and interaction, with reusable logic/state moved into appropriate modules.
- Keep reusable UI primitives in `src/lib/components/ui` and feature-specific components with their feature.
- Derive state instead of duplicating it and avoid unnecessary reactive state.
- Keep Tauri `invoke()` calls behind dedicated frontend service modules rather than scattering them through components.

## UI

- Use Tailwind CSS and existing design tokens, spacing, typography and UI primitives before introducing arbitrary values.
- Turn genuinely repeated UI patterns into reusable components, but do not abstract one-off visual details.
- Keep equivalent controls visually and behaviorally consistent.
- Avoid unnecessary animation and respect reduced-motion preferences where relevant.

## Comments

- Comments are short factual notes, not explanations of how or why code supposedly works.
- Comment only non-obvious constraints, caveats, external quirks, decisions or temporary limitations; never narrate obvious code.
- Prefer clearer code over explanatory comments and never use comments as evidence that code is correct.
- Keep comments local and verifiable; update or remove stale comments when changing related code.
- TODOs must describe a concrete remaining task.

## Errors and Logging

- Never silently ignore errors; expose useful user-facing messages while preserving technical details needed for debugging.
- Log meaningful state changes and failures, not routine function calls or hot-path noise.
- Never log passwords, tokens, secrets or sensitive user data.
- Logging never replaces proper error handling.

## Dependencies and Structure

- Add maintained dependencies only when they provide clear value and existing project code or dependencies do not already solve the problem; avoid dependencies for trivial helpers and remove unused ones.
- Put code where its responsibility belongs; avoid catch-all files such as oversized `utils.ts` or `helpers.rs`.
- Prefer domain-specific modules, small public APIs and no circular frontend dependencies.
- Do not create empty architecture for future needs.
- Keep generated files out of source control unless explicitly required.

## Security

- Never commit secrets, tokens, credentials or private keys.
- Treat remote manifests, API responses, downloaded files and frontend-provided paths as untrusted and validate them.
- Verify downloads with hashes or signatures when available.
- Grant only required Tauri capabilities and native permissions.
- Never construct shell commands from untrusted strings; prefer structured process arguments.

## Changes and Tests

- Understand existing code before changing it and preserve current behavior unless the task explicitly changes it.
- Do not leave dead code, commented-out code, temporary debug output or unused dependencies.
- Do not consider work complete while relevant checks are failing.
