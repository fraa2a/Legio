# Legio implementation roadmap

`PLAN.md` is the immutable canonical product specification. `AGENTS.md` is binding for engineering and repository practices. These phase documents turn the canonical requirements into an executable roadmap without replacing or weakening them.

Frontend UI implementation docs: [TODO](../frontend/TODO.md) and [developer guide](../frontend/backend-contracts.md). Keep both current as views and backend contracts evolve.

## Status vocabulary

- `Planned`: scoped, but implementation has not started.
- `In progress`: implementation or required verification is underway.
- `Blocked`: a named external input or prerequisite prevents completion.
- `Completed`: every completion criterion has current evidence.
- `Deferred`: intentionally postponed by an explicit decision.

## Phase index

| Phase | Status | Dependencies | Next uncompleted milestone |
| --- | --- | --- | --- |
| [01 Foundation and application shell](01-foundation-and-application-shell.md) | Completed | None | 02A SQLite initialization and migrations |
| [02 Local state, settings, and library](02-local-state-settings-and-library.md) | Completed | 01 | 04A source manifest validation |
| [03 Steam integration and catalog](03-steam-integration-and-catalog.md) | Completed | 02 | 04A source manifest validation |
| [04 Legio source, Store, and trust](04-legio-source-store-and-trust.md) | Completed | 03 | 05A queue controls and restart verification |
| [05 Downloads, installation, and import](05-downloads-installation-and-import.md) | In progress | 04 | 05A queue controls and restart verification |
| [06 Launch, playtime, and offline](06-launch-playtime-and-offline.md) | In progress | 05 | 06B Steam and native Windows runtime verification |
| [07 Linux compatibility and OFLL](07-linux-compatibility-and-ofll.md) | In progress | 06 | 07C implement canonical-compatible backend parity gaps |
| [08 Product UI and operations](08-product-ui-and-operations.md) | Blocked | 02, 03, 04, 05, 06, 07 | Supply the section 35 mockup, then 08A product UI |
| [09 Updates, release, and future](09-updates-release-and-future.md) | Planned | 04, 05, 06, 07, 08 | 09A game update policies |

Frontend implementation guide: [backend contracts](../frontend/backend-contracts.md). Keep it synchronized with Tauri command registration and frontend service types when implementing or changing UI flows.

## Dependency graph

```text
01 -> 02 -> 03 -> 04 -> 05 -> 06 -> 07
      \_______________________________/
                     |
                     v
                    08
              04..08 |
                     v
                    09
```

Phase 08 depends on Phases 02 through 07. Phase 09 depends on Phases 04 through 08.

## Update protocol

Update the relevant phase document when any of these events occurs:

- a phase or milestone completes;
- an architectural decision changes;
- a new dependency appears;
- a requirement needs clarification;
- implementation must diverge from `PLAN.md`.

A canonical product requirement must not be silently rewritten. Before implementation diverges, record the problem, proposed replacement, approval state, date, and affected milestones under `Open technical decisions` or `Update notes`. Keep unresolved changes visibly open.

For every completed milestone, record evidence in this format:

- **Platform:** operating system and relevant environment.
- **Command or scenario:** exact command or exercised user flow.
- **Observable result:** output or behavior that proves the milestone.
- **Remaining blockers:** any unmet completion criterion, or `None`.

Required GitHub hosting action: protect `main` and require the `frontend`, `rust`, and `tauri-smoke` checks. Repository settings are not changed by this roadmap task.

## Persistent guardrails

- No mandatory accounts, cloud services, or telemetry.
- No plugin ecosystem, arbitrary user-provided source providers, or arbitrary source-defined scripts.
- The project uses one project-controlled Legio source.
- Automatic matches and metadata remain visible and user-editable.
- No torrents, encrypted archives, or multipart archives.
- Rust owns business logic, filesystem access, processes, downloads, installation, and system integration.
- Svelte owns presentation, interaction, and frontend state. The UI never owns installation logic.

## Canonical section coverage

Each section has exactly one owning phase. Prerequisites identify cross-cutting dependencies without creating a second owner.

| Section | Canonical topic | Owner | Prerequisites |
| --- | --- | --- | --- |
| 1 | Project definition | 01 | None |
| 2 | Product principles | 01 | None |
| 3 | Platform support | 07 | 01, 06 |
| 4 | Product experience | 08 | 02 through 07 |
| 5 | Steam integration overview | 03 | 02 |
| 6 | Local data and persistence | 02 | 01 |
| 7 | Steam local detection | 03 | 02 |
| 8 | Legio source overview | 04 | 03 |
| 9 | Manifest structure | 04 | 03 |
| 10 | Manifest validation | 04 | 03 |
| 11 | Source caching and merge | 04 | 03 |
| 12 | Download system | 05 | 04 |
| 13 | Product UI details | 08 | 02 through 07 |
| 14 | Download queue behavior | 05 | 04 |
| 15 | Archive verification | 05 | 04 |
| 16 | Safe extraction | 05 | 04 |
| 17 | Executable detection | 05 | 04 |
| 18 | Installation finalization | 05 | 04 |
| 19 | Steam metadata and artwork | 03 | 02 |
| 20 | Trust and availability presentation | 04 | 03 |
| 21 | Game updates | 09 | 04, 05 |
| 22 | Application updates | 09 | 08 |
| 23 | Launching | 06 | 05 |
| 24 | Playtime | 06 | 05 |
| 25 | Linux compatibility | 07 | 06 |
| 26 | Compatibility configuration | 07 | 06 |
| 27 | Offline behavior | 06 | 03, 05 |
| 28 | OFLL parity | 07 | 06 |
| 29 | Settings and library state | 02 | 01 |
| 30 | Networking | 03 | 02 |
| 31 | Cache and connectivity | 03 | 02 |
| 32 | Technology stack | 01 | None |
| 33 | Diagnostics | 03 | 02 |
| 34 | Installation recovery | 05 | 04 |
| 35 | Visual mockup | 08 | Mockup input, 02 through 07 |
| 36 | Achievements | 09 | Explicit future decision |
| 37 | Optional accounts | 09 | Explicit future decision |
| 38 | Repository and tooling | 01 | None |
| 39 | Release system | 09 | 08 |
| 40 | Initial development objective | 01 | None |
| 41 | Development order | 01 | None |
| 42 | Core definition of done | 09 | 01 through 08 |
| 43 | Architectural boundaries | 01 | None |
| 44 | Non-goals | 01 | None |
| 45 | Final target | 09 | 01 through 08 |

## Section 42 core definition of done

Keep every item open until its observable behavior is proven on the stated platforms.

- [ ] run on Windows and Linux.
- [ ] detect Steam and installed Steam games.
- [ ] import Steam games into Legio.
- [ ] search the Steam catalog.
- [ ] show Steam metadata and artwork.
- [ ] read the Legio JSON source.
- [ ] distinguish verified and unverified game entries.
- [ ] show clear unverified warnings.
- [ ] download supported archives.
- [ ] pause/resume/retry downloads.
- [ ] recover the queue after restart.
- [ ] verify SHA256 before installation.
- [ ] safely extract supported archives.
- [ ] identify the correct game executable or ask the user.
- [ ] automatically add successful installations to the library.
- [ ] add arbitrary games manually.
- [ ] enrich manual games when Steam identity is detected.
- [ ] allow users to override automatic metadata.
- [ ] launch games reliably.
- [ ] track playtime.
- [ ] provide Home and Library views.
- [ ] work offline for local functionality.
- [ ] run supported Windows games on Linux through the compatibility subsystem.
- [ ] reproduce the required Online Fix Linux Launcher capabilities in Rust.
- [ ] expose useful diagnostics when launch/compatibility fails.
