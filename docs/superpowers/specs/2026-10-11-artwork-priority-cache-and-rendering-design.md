# Artwork priority, cache freshness and stable rendering

Date: 2026-10-11

Status: Written specification approved by the user on 2026-10-11; implementation plans await review and execution-method selection.

Baseline: `feat/store-filters-release-0.2.0` at `733b0a5`, draft PR #183.

## Purpose and scope

Make Store and Library feel responsive while images are loading. Opening a game must give its assets priority over list images and speculative prefetch. Previously cached content must remain visible during updates. Correct the reported live dialog-blur fragmentation, navigation flashes and Dither composition defects without regressing readable text or the resolved Windows stack overflow.

The user approved this direction on 2026-10-11. Work remains on the existing release branch. PR #183 stays draft until the user accepts the Windows installer. TaxLauncher migration and release/merge actions remain subsequent work. The other developer's transparency issue is explicitly deferred.

## Evidence and limits of the investigation

- Image requests are deduplicated but have no global network concurrency limit or selected-game promotion. Conversion uses a FIFO semaphore with two permits.
- Store rows request metadata, a header and a blurred hero. A blurred hero can require fetching and converting a sharp hero, then decoding and converting the derived background. Visibility is observed once, so work can accumulate as the user scrolls.
- The Library detail banner currently waits for metadata even when its hero could load independently.
- Metadata already persists in SQLite, with a one-day freshness threshold. Images already persist on disk, with a three-day threshold. Image consumers can trigger automatic refresh from lists and timers.
- Automatic image refresh revokes the old object URL before confirming that the replacement can be displayed. Manual refresh resets persistent artwork before downloading replacements.
- Sidebar is outside the keyed page subtree and does not remount on normal navigation. Native navigation nevertheless crossfades the entire document root, including sidebar, logo and background.
- Dialog blur is applied to the dialog containing its content and interpolated for 180 ms while the slider changes. Its content has persistent `will-change: opacity`, with nested glass panels.
- Dither uses WebGL2; the working particles effect uses a 2D canvas. Dither already requests a persistent drawing buffer and writes opaque pixels. Transparent patches are not intended shader output. There are no context restoration handlers or framebuffer-completeness checks.

These observations establish scheduling and lifecycle problems. They do not establish which phase dominates Windows loading time or prove the cause of the Dither corruption. Existing JSDOM and mocked graphics tests cannot establish native GPU rendering correctness. The implementation must gather that evidence before selecting a Dither correction.

## Native scheduling and priorities

Rust owns scheduling, network limits, conversion limits and persistent cache operations. Svelte declares the current game and active artwork consumers through service modules; it does not implement a second download queue. Tauri commands stay thin.

Use the following priority order:

1. Assets and metadata needed by the open game, with banner/cover and logo ahead of its optional gallery work.
2. Primary covers and necessary metadata for visible Store/Library entries.
3. Decorative list backgrounds, speculative prefetch and work with no current visible consumer.

Allow at most five concurrent artwork-related network operations. Ordinary list/background work can occupy at most four slots; reserve the fifth for the current game. Include Steam metadata resolution, including PICS lookups, in the bounded network work. Do not put unrelated game-package downloads in this scheduler. A lookup that involves several protocol steps occupies one network operation, rather than recursively acquiring permits for its internal steps.

Keep at most two concurrent image conversions. Admit ordinary work into one conversion slot; reserve the other for the current game. The current game may use available capacity within those global limits. These limits deliberately guarantee urgent capacity rather than increasing the total when the ordinary queue is busy.

Deduplicate by resource identity and transformation profile. Priority is scheduling information, not part of the cache or deduplication key. Selecting a game promotes its existing queued work and any future stages of work already in progress. A request already transferring is reused, not downloaded again. Opening a different game transfers urgent status to that game; finishing an obsolete request must not replace the new game's UI. If obsolete urgent network work occupies the reserved capacity, cancel enough of that work to admit the new game's request. Requeue canceled resources only if they retain a consumer or prefetch purpose; cancellation must release capacity and must not publish a partially downloaded cache entry.

Do not interrupt a native image conversion halfway through its codec call. If an obsolete game's conversion already occupies the reserved slot, the newly selected game takes the next available conversion slot ahead of queued work. The global two-conversion bound still applies while obsolete conversions finish. In-progress ordinary transfers can finish and populate the cache while urgent work uses reserved capacity. Unstarted work with no remaining consumer or prefetch purpose is removed. Remaining work resumes as capacity becomes available, with FIFO order within each priority class.

Acquire capacity at the network and conversion stages, not around an entire recursively composed asset request. Deriving a blurred hero must not hold a permit while waiting for another asset that needs the same permit. Disk/memory cache hits bypass the network and conversion queues.

## Cache freshness and update triggers

| Resource | Freshness | Missing entry | Existing entry |
| --- | --- | --- | --- |
| Localized game metadata | 3 days from successful fetch | Fetch when a visible list entry or game page needs it | Read immediately; refresh if expired when the game page opens |
| Steam artwork, including covers, library heroes and derived variants | 5 days from successful fetch | Fetch when needed by a visible consumer or prefetch | Display immediately; refresh if expired when the game page opens |

The first visit to Store or Library may populate the cache. Merely displaying a cached entry there never refreshes it, even after expiry. There is no periodic refresh timer for unopened games and no refresh solely because the application restarts or a list is scrolled. A page left open beyond the threshold is checked on its next opening, not by a background timer. Explicit user refresh remains available and follows the same safe replacement rules.

Freshness advances after a successful fetch, not after reading the cache, opening a page or failing a request. Check thresholds using elapsed time in the appropriate stored units. A future timestamp caused by clock changes is eligible for revalidation on opening; it must not hide otherwise valid cached content.

Preserve localized metadata cache identities. Changing language may fetch missing localized metadata, but must not invalidate language-independent artwork. Updating metadata after three days must not invalidate an image merely by changing `cachedAt`, since its separate five-day policy still applies.

Retain existing bounded storage policies. Expiry means eligible for refresh, not eligible for immediate deletion. Normal storage eviction and unusable/corrupt cache entries can still result in a cache miss; the always-visible guarantee applies whenever a valid previous entry exists. Existing cache files and database records remain readable without a destructive migration.

The page-open check covers the cached artwork variants belonging to that game, including those initially loaded by a list. Essential page assets take precedence over optional variants. Concurrent opens/refreshes share existing work. Metadata dependencies for headers and screenshots must be resolved before requesting those resources; a hero does not wait for unrelated textual metadata.

## Replacement and failures

Read and publish the last valid metadata/image immediately, even when expired. Fetch and process the replacement separately. For images, complete native validation and effects, then confirm that the replacement can be decoded for display before publishing it.

Keep the previous displayed image and its object URL retained until the new image is ready and has replaced it. Revoke an old URL only after its remaining consumers release it. A shared consumer still displaying an old generation must remain valid. Do not briefly reset the component to an empty placeholder on metadata changes or refresh start.

Write validated native replacements through temporary files and atomic replacement. Retain the previous persistent generation until replacement validation succeeds; for an active image consumer, also retain it until display decode succeeds. Native-only prefetch can complete publication after native validation. If all consumers leave before display validation, validated native output can finish as prefetch. Cleanup of superseded persistent generations must respect pending publication; retained object URLs independently protect consumers still showing an older image. Do not call destructive cache reset before automatic or explicit refresh. A failed download, unsupported image, conversion failure or failed display decode preserves the last valid image. A failed metadata refresh preserves the last valid metadata. Report failures through existing error and opt-in logging paths. A failed fetch does not mark stale content fresh.

Retry failed scheduled updates on a later game-page opening, with a short shared retry backoff to prevent rapid navigation from flooding the service. Preserve the current five-minute retry backoff; it is independent of the three/five-day successful-fetch thresholds. Explicit user refresh can retry immediately.

## Dialog, navigation and Dither rendering

Separate the dialog's decorative blur/background surface from the foreground containing text and controls. Blur remains live as the slider moves; do not defer preview until pointer release. Avoid continuously interpolating `backdrop-filter` on an ancestor of the settings content, and remove permanent opacity promotion where it creates an unnecessary backdrop boundary. Preserve dialog opening/closing behavior, keyboard dismissal, focus handling and reduced-motion support.

Navigation must leave sidebar, logo, title bar and the application background visually stable. Fade the changing page's foreground without fading the entire window or disconnecting glass surfaces from their intended background. Do not simply assign a transition name or opacity animation to a common ancestor of all glass panels: both can create a backdrop root. Keep blur-bearing surfaces stationary relative to the content animation. Reuse the existing layout and CSS conventions; do not add a generic rendering framework.

For Dither, first isolate shader output from composition: record context loss/restoration and framebuffer status, compare canvas pixels with the presented frame, and compare animated/frozen Dither with particles and no effect. Compare navigation with and without the native snapshot path. Keep diagnostic details in the existing opt-in application log, without per-frame logging.

Maintain valid output through resize, pause/resume and context recovery. Reinitialize native GL resources after restoration and retain a valid visible frame or the configured background while output is unavailable. Changing an unrelated appearance property must not restart or reschedule a frozen Dither renderer unnecessarily.

Keep the Dither appearance and its controls. Do not globally disable graphics acceleration or silently replace the selected effect as the fix. Select the smallest correction supported by reproduced evidence. A change that requires replacing the renderer or removing an approved animation requires a focused design revision before implementation.

## Diagnostics and validation

Application and network logging remain separate, opt-in and disabled by default. Add useful operation summaries distinguishing queue wait, metadata resolution, transfer, conversion and publication. Use resource kinds and non-sensitive request identifiers. Logging must not become a prerequisite or awaited barrier for drawing an image. Avoid logging URLs, local paths, credentials or graphics output every frame.

Automated validation must cover:

- A saturated ordinary queue never exceeds four active network operations; opening a different game starts its urgent request without waiting for the ordinary queue to drain. Overall work never exceeds five network operations or two conversions.
- Queued work for a selected game is promoted, duplicate requests share work, changing games releases obsolete urgent network capacity, obsolete consumers cannot overwrite current UI, nested hero/blur requests do not deadlock, and background work continues after urgent work completes.
- Memory/disk cache hits do not wait for network or conversion capacity.
- Metadata and artwork threshold boundaries, warm restart, localization, clock rollback, and the absence of list/timer-based expired-cache refresh.
- Previous metadata and images remain visible during successful and failed refresh; one consumer's update/release does not revoke another consumer's displayed URL. Explicit refresh does not erase the last usable persistent entry first.
- Sidebar and canvas identity survive navigation, live dialog preview keeps foreground content sharp, and Dither resources recover from simulated context loss. GPU mock tests are explicitly insufficient for visual acceptance.
- The existing release-profile command-future size checks continue passing; preserve boxed asset futures and the Windows stack-overflow regression protection.

Measure representative cold-cache and warm-cache loading separately on the same Windows installation, with consistent viewport, games and network conditions. Record time to the first visible cover, essential selected-game artwork, and complete visible rows, with logging overhead reported separately. Do not claim a fixed latency or competitor equivalence without measurements.

Windows manual acceptance includes Store and populated Library navigation, selecting a game while list work is queued, old-cache refresh while offline, repeated dialog-blur dragging, and repeated page fades using Dither, particles and no background. Include the user's reported 15% panel opacity and 6 px panel blur, expanded/collapsed sidebar, resize/fullscreen and reduced motion. Inspect frames during transitions, not only settled screenshots.

## Handoff and release condition

After user review of this written specification, prepare the implementation plan and its execution-method handoff according to Superpowers. Implement and verify the two focused work areas separately so scheduling changes do not obscure graphics regressions. Publish a new Windows test installer from the updated draft PR only after relevant checks pass. Keep the release merge and TaxLauncher migration pending the user's Windows acceptance.

## Rendering references

- [CSS Filter Effects: backdrop roots](https://drafts.csswg.org/filter-effects-2/#BackdropRoot).
- [CSS View Transitions: rendering consolidation](https://drafts.csswg.org/css-view-transitions-1/#rendering-consolidation).
