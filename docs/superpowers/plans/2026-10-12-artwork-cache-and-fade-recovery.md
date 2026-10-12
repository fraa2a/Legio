# Artwork cache and foreground fade recovery

Release 0.2.0 and the TaxLauncher migration remain suspended. This change prepares a draft PR test installer only.

## Requirements

- Keep Library blurred covers, icons, logos, capsules and headers on disk until explicit refresh or reset.
- Refresh textual metadata after three days and sharp detail artwork after five days only when opening game details. Display existing images during replacement.
- Prioritize missing visible artwork before background refreshes, with selected-game visible requests first among visible misses.
- Bound the image pipeline through download, conversion, blur and cache publication. Keep capacity for selected-game demand.
- Stop outgoing-page and offscreen demand while preserving images during the foreground fade.
- Keep native Skia composition and protected glass surfaces stable.

## Implementation and evidence

1. Native cache: permanent classes are excluded from age expiry and automatic pruning. A blur derived from an old cached sharp image is persisted. Regression tests cover reopening the disk cache.
2. Scheduler: at most five image/network slots, with one reserved for selected demand; two codec slots without a selected game. Image slots survive conversion and publication. Shared image and metadata owners inherit joining visible demand. Visible misses precede refreshes.
3. Presentation: page and viewport scopes release demand before outgoing fade completes. Library tiles no longer request textual metadata. Global startup hero prefetch is removed. Permanent classes bypass automatic detail-open refresh while explicit refresh still works.
4. Fade: underlying opacity is set to its terminal value before Web Animations. Detached outgoing layers remain transparent. Existing CSS opacity and connected-node cancellation are preserved.
5. Final review: continuous sidebar observation and cancelled metadata negative caching were corrected with failing regression tests followed by passing tests. Metadata ownership is registered before the first asynchronous cache read.

## Validation limits

The isolated native WebKitGTK 2.54.1 X11 probe showed monotonic outgoing/incoming alpha after the fix. Wayland opened successfully, but native window readback could not verify pixels. Full application behavior on Hyprland and Windows still needs user acceptance. No claim is made that every driver or compositor configuration is fixed.

The Windows installer must be built from this PR head, with build-info and SHA256 verified. No merge, release or production update is authorized while testing is pending.

## Final validation

- Native suite: 466 passed, four pre-existing ignored live/platform tests; no failures.
- Clippy: all targets and features, warnings denied, passed.
- Svelte check: no errors or warnings. ESLint and production frontend build passed.
- Focused Library rendering tests: 15 passed, including page cancellation, retained bitmap and continuous sidebar observation.
- Image cache tests: 21 passed, including permanent assets and explicit refresh.
- Metadata cancellation regression failed before the fix and passed afterwards.
- The initial sandboxed native run could not bind local test servers or write an installed-profile test path. The rerun with those permissions passed.
