# Frame-driven navigation and unavailable updater platform

The maintainer confirmed Windows acceptance for 219ece2, then confirmed Hyprland still flashes with every background, including None. They explicitly requested removing the current animation and rebuilding it. The release and TaxLauncher migration remain suspended.

## Design

Replace the entire compositor-animation implementation with one requestAnimationFrame clock for each outgoing/incoming phase. Every foreground target receives an ordinary style opacity update in the same paint frame. No Web Animations effects, whole-document snapshots, transforms or extra blur filters are created. Keep the established protected-glass target selection, navigation request guards, demand cancellation and reduced-motion behavior. Cancellation restores original inline opacity and priority for connected nodes; retired outgoing nodes stay transparent.

The renderer follows normal style invalidation and painting in WebKit's Skia pipeline. This is a compatibility approach, not a claim that WebKit mandates this API or that its underlying regression has been repaired.

## Updater error

The installed updater plugin reports TargetNotFound/TargetsNotFound when the feed has no matching platform. Represent that as unavailable and show a readable platform availability message. Do not report the application as current. Clear an earlier available installer when checking again so a failed check cannot install stale content. Preserve real network/manifest errors.

## Evidence and verification

- New regression tests failed against the old compositor animation, then passed with the frame clock.
- Tests cover midpoint/end opacity, one clock across targets, protected glass, CSS opacity/priority, cancellation and obsolete navigation.
- Updater regression failed before implementation and passed afterwards, including stale installer invalidation and preserved network errors.
- Independent final review found no important concrete issue. Visible Hyprland acceptance remains required.
- Linux test artifacts use an optimized release build with the embedded production frontend, matching the Windows build profile. The earlier Linux debug artifact is unsuitable for direct codec-performance comparison.
- Diagnostic native sessions use isolated database/cache copies. No original profile is modified.

## Completed local validation

- All frontend test scripts passed, including application logging adapted to the explicit frame clock.
- Svelte check: zero errors and warnings. ESLint and production build passed.
- Optimized native Linux application with embedded frontend built successfully.
- A real Hyprland Wayland session was verified through its process/window backend, with background None and an isolated profile. Physical window captures showed one outgoing fade followed by one incoming fade, without a return to the outgoing page in that run. Navigation generated no native compositor animation effects. This is limited local evidence, not acceptance on every configuration.
- The updater settings showed platform unavailability, without the raw fallback-platform error and without claiming the application is current.
- Windows acceptance of this new animation remains separate from the earlier 219ece2 acceptance.
