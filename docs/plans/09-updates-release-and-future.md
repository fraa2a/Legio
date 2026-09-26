# Phase 09: Updates, release, and future

## Status

Planned

## Canonical requirements

Owns `PLAN.md` sections 21, 22, 36, 37, 39, 42, and 45.

## Objective

Complete safe game and launcher updates, produce release artifacts, verify the core product definition of done, and keep achievements and accounts behind explicit future decisions.

## Scope

Game version policies, rollback-safe updates, independent launcher updates, Windows and Linux release channels and metadata, final core acceptance, deferred achievements, and optional additive accounts.

## Milestones and principal tasks

### 09A: Game update policies

- Compare installed and available game versions using a documented ordering rule.
- Support automatic, ask, manual, and never policies with per-game overrides.
- Stage updates and retain rollback-safe recovery.

Verification: upgrade, downgrade, equal, malformed, interrupted, and rollback scenarios preserve a usable installed version and honor policy precedence.

### 09B: Launcher updates

- Verify launcher updates independently from game updates and source refresh.
- Present availability, progress, failure, and restart requirements clearly.
- Preserve local functionality when update services are unavailable.

Verification: valid, invalid, offline, interrupted, and no-update scenarios cannot corrupt the installed launcher.

### 09C: Release system

- Produce Windows and Linux installers or packages.
- Publish checksums and appropriate release metadata.
- Support development builds, prereleases, and stable releases.
- Keep Cargo package version as the sole version authority.

Verification: clean Windows and Linux environments install, launch, and identify the expected version; published checksums verify every artifact.

### 09D: Deferred achievements

- Implement achievements matching section 36 only after a dedicated approved decision.
- Keep the feature independent from core launch and library behavior.

Verification: defined only after approval, with observable requirements added here before implementation.

### 09E: Optional additive accounts

- Implement accounts only after an explicit future decision.
- Keep local use fully functional without an account.
- Make synchronization additive rather than authoritative over local data.

Verification: defined only after approval, including account deletion and offline behavior, before implementation.

## Dependencies

Phase 09 depends on Phases 04 through 08. Core definition-of-done closure depends on evidence from all preceding phases.

## Completion criteria

Game and launcher updates are verified and recoverable, Windows and Linux releases include checksums and metadata, every section 42 checklist item has evidence, and deferred features remain absent unless explicitly approved.

## Open technical decisions

- Record a game-release version ordering rule before 09A.
- Define launcher signing and update metadata trust before 09B.
- Decide platform packaging formats and signing before 09C.
- Achievements remain deferred pending a dedicated decision.
- Accounts remain conditional on an explicit future decision and may never become mandatory.

## Update notes

09C release automation follow-up (2026-09-25): the publish job now creates a deterministic `SHA256SUMS.txt` for Linux and Windows bundle files, verifies it before publication, and attaches it with flat bundle assets to the GitHub release. Users can verify downloaded files together with `sha256sum --check SHA256SUMS.txt`. Package signing and clean-platform installation acceptance remain open.
