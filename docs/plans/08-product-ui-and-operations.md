# Phase 08: Product UI and operations

## Status

Blocked

## Canonical requirements

Owns `PLAN.md` sections 4, 13, and 35 as the final cross-product UI and operations integration owner.

## Objective

Unify the working backend slices into the complete desktop product experience, then add notifications and Rust-owned queue-completion power actions.

## Scope

Home, Library, Browse and Store, Spotlight-only catalog search, Game, Downloads, Settings, activity visualization, action states, remote-description sanitization, desktop notifications, and queue-completion actions.

## Milestones and principal tasks

### 08A: Complete product experience

- Integrate Home, Library, Browse and Store, Game, Downloads, and Settings with existing backend slices.
- Keep catalog search in the Spotlight-style Store search interaction only.
- Include every section 4 library control and setting.
- Render an approximately 30-day activity heatmap from local sessions.
- Implement all section 20 action states.
- Sanitize remote descriptions at their first rendering boundary.
- Match section 35 visual direction after its missing mockup is supplied.

Verification: every screen and control drives real state, keyboard and pointer paths are usable, remote descriptions cannot inject active content, and visual review uses the supplied mockup.

### 08B: Desktop notifications

- Notify only for canonical user-relevant events.
- Respect user settings and platform permission state.
- Avoid duplicate or noisy notifications.

Verification: enabled, disabled, denied, and repeated-event scenarios behave distinctly on Windows and Linux.

### 08C: Queue-completion actions

- Define exact queue-completion semantics before implementation.
- Execute do nothing, shutdown, and supported sleep or suspend actions in Rust.
- Require a clear user choice and prevent accidental action when work remains or fails.

Verification: each action executes only after the defined successful terminal queue state, cancellation prevents it, and unsupported platform actions are unavailable.

## Dependencies

Phases 02 through 07 must provide the real data, source, download, launch, offline, and compatibility slices. Visual parity also requires the missing section 35 mockup.

## Completion criteria

All product surfaces operate on real backend state, visual parity is reviewed against the supplied mockup, remote content is sanitized, notifications honor settings, and queue-completion actions follow defined safe semantics.

## Open technical decisions

- Blocking visual parity: the section 35 mockup is absent and must be supplied.
- Define queue-completion success, mixed-result, cancellation, and restart semantics before 08C.
- Confirm per-platform notification permission behavior before 08B.

## Update notes

- 2026-09-21: Phase recorded as blocked because the canonical visual mockup is not present. Backend-dependent planning may continue, but final visual parity may not be claimed.
