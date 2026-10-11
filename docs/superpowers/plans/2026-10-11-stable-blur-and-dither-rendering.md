# Stable Blur and Dither Rendering Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Keep dialog text sharp during live blur changes and retain stable sidebar/background/glass rendering through navigation, with evidence for the native Dither defect.

**Architecture:** A stationary decorative dialog surface owns blur; foreground entrance/exit is separate. Page transitions animate only safe foreground subtrees, never the document root or an ancestor of blur-bearing content. Dither gains measured lifecycle diagnostics/recovery; any corruption correction follows native reproduction rather than speculative shader changes.

**Tech Stack:** Svelte 5, CSS, Web Animations API, existing WebGL2 renderer, Node 22/JSDOM tests, Windows Tauri/WebView2 acceptance.

**Spec:** [Approved design](../specs/2026-10-11-artwork-priority-cache-and-rendering-design.md).

## Global Constraints

- Retain version `0.2.0`, existing release branch and draft PR #183. No merge before user Windows acceptance; defer TaxLauncher migration and the other developer's transparency issue.
- Preserve live slider preview, dialog open/close/focus/keyboard handling, Dither controls and visual style, and reduced-motion support.
- Sidebar, logo, title bar and application background remain visually stable during navigation. Glass surfaces retain their background access.
- Do not replace Dither, remove the approved page fade, globally disable hardware acceleration, or add a generic rendering framework/dependency as a workaround.
- Logging remains opt-in, disabled by default, bounded and free of sensitive paths/URLs or per-frame output.
- JSDOM/mocked WebGL checks do not establish native Windows visual correctness. No unsupported GPU cause or fixed claim.
- Reuse CSS/layout conventions; Git metadata/documentation use English and no em/en dashes. Preserve the artwork command stack-overflow regression fix.

## Review Focus

- Rapid navigation during exit/entrance with a selected game: latest target wins, obsolete page never reappears, and artwork priority is not delayed by the fade. Task 2.
- Live dialog blur while opening/closing or dismissing with Escape: text remains readable, close completion fires once and focus/lifetime behavior remains correct. Task 1.
- A nested or Tailwind blur surface missed by transition targeting: no animated ancestor cuts off backdrop access. Task 2.
- GL context loss or resize while hidden/frozen: resources recover without an extra frame loop or transparent/invalid output. Task 3.
- WebGL pixels remain correct while the compositor corrupts a snapshot: preserve the evidence and avoid changing shader mathematics as an unsupported fix. Tasks 3 and 4.

## File Map and Execution Order

Modify `src/lib/components/ui/Dialog.svelte`, `src/app.css` and `scripts/test_dialog.mjs` for dialog layering. Create `src/lib/utils/foreground-transition.ts` for narrowly scoped DOM animation; modify `MainContainer.svelte`, `motion.ts` and `scripts/test_visual_rendering.mjs`. Modify `DitherBackground.svelte` and `AppBackground.svelte`; create `src/lib/components/layout/dither-renderer.ts` to own the existing GL resource lifecycle rather than duplicating it. Extend `scripts/test_appearance.mjs` and visual tests. Update Windows acceptance documentation and existing CI script coverage. This plan is independently testable; run after the artwork plan for the combined test installer.

## Task 1: Stationary Dialog Blur and Live Preview

**Files:** Modify `Dialog.svelte`, `src/app.css`, `scripts/test_dialog.mjs`, `scripts/test_appearance.mjs`; modify `SettingsGroup.svelte` only if nested-surface evidence requires it.

**Interfaces:** Keep all existing Dialog props and snippet APIs. Add a decorative `div.legio-dialog-surface[aria-hidden=true]` before `div.legio-dialog-content`; expose no new public component props. Keep `data-visible` on the dialog for open/close state. Target the content explicitly by class in close-completion handling rather than assuming `firstElementChild`.

- [ ] **Write failing tests** `live_blur_targets_surface_not_foreground`, `dialog_close_waits_only_for_content_transition`, and `blur_drag_during_close_notifies_once`. Change blur variables repeatedly while open/closing; assert the dialog/content have no backdrop filter or permanent `will-change: opacity`, the decorative surface owns the live filter, and close does not react to surface or nested transition events. Preserve reopen-during-close and Escape/cancel tests. Do not use source-string assertions as a substitute for mounted lifecycle checks.
- [ ] **Run red:** `node --conditions=browser --test scripts/test_dialog.mjs scripts/test_appearance.mjs`. Expect failures against the old filter owner and first-child close assumption.
- [ ] **Implement stationary layering.** Keep dialog opacity at one; decorative surface is positioned behind the relative foreground, clipped to the dialog radius, and does not receive pointer events or continuously interpolate backdrop-filter. Content retains its existing 180 ms entrance/exit but no permanent promotion after settling. Preserve top-layer visibility behavior and focus. Do not disable the slider or postpone preview until release. Keep nested panel blur changes separate until the layered dialog test identifies whether they remain necessary.
- [ ] **Run green** plus frontend check/lint. In a real browser inspect computed filter ownership during drag; record a native Windows drag/open/close sequence for visual acceptance. If the surface approach reproduces the earlier bloom defect, stop and isolate that failure before proceeding rather than combining more CSS guesses.
- [ ] **Commit:** `fix: isolate live dialog blur from settings foreground`.

## Task 2: Foreground-Only Page Fades

**Files:** Create `src/lib/utils/foreground-transition.ts`; modify `MainContainer.svelte`, `motion.ts`, `src/app.css`, `scripts/test_visual_rendering.mjs`. Keep `App.svelte`, Sidebar and background ownership unchanged.

**Interfaces:** `collectForegroundTargets(root: HTMLElement): HTMLElement[]` returns non-overlapping safe foreground subtrees. `animateForeground(root: HTMLElement, direction: "in" | "out", duration: number): { finished: Promise<void>; cancel(): void }` uses those targets. Total navigation duration stays 210 ms: 105 ms outgoing foreground, swap, 105 ms incoming foreground; reduced motion uses zero. MainContainer keeps a monotonically increasing navigation-request guard and cancels obsolete animation handles.

- [ ] **Write failing tests** `navigation_keeps_sidebar_and_canvas_nodes`, `foreground_targets_never_contain_backdrop_surface`, `rapid_navigation_never_republishes_obsolete_target`, and `logging_does_not_gate_page_swap`. Mount representative nested `.legio-glass`, `.legio-panel`, a computed Tailwind-like backdrop filter, and plain text/artwork. Assert every animated target has no own/descendant blur surface, no target is an ancestor of another target, stationary surface opacity stays one, and only the latest navigation swaps content. Block logging promises and confirm navigation still completes. Reduced motion must create no timed entrance/exit.
- [ ] **Run red:** `node --conditions=browser --test scripts/test_visual_rendering.mjs`. Existing whole-document snapshot expectations must be replaced by behavior checks for the approved stable-background contract.
- [ ] **Implement safe targeting and transition ownership.** Walk the mounted page once, computing whether a subtree contains protected surfaces. Protect `.legio-glass`/`.legio-panel` even when their current blur is zero, plus elements with non-none computed `backdrop-filter`/`-webkit-backdrop-filter`; keep their ancestors unanimated and descend to eligible foreground children. Animate eligible subtrees via Web Animations API. Do not animate text nodes without an element owner. Remove native document-root snapshot crossfade CSS/calls and the full page-wrapper opacity entrance. Keep visible old surfaces until the exit finishes, swap with `flushSync`, then animate the new foreground. No `view-transition-name` is placed on a common page ancestor. Cancel safely during teardown and rapid navigation; fire diagnostics without awaiting them. The artwork plan declares urgent selection before this page delay.
- [ ] **Run green**, then browser frame inspection with sidebar expanded/collapsed and panel opacity 15%, blur 6 px under Dither, particles and no background. Confirm protected surfaces retain blur before/during/after each phase. If safe targeting would require replacing the renderer or removing fade, follow the spec's focused design-revision gate rather than silently broadening scope.
- [ ] **Commit:** `fix: fade page foreground without snapshotting application chrome`.

## Task 3: Dither Lifecycle and Corruption Evidence

**Files:** Create `src/lib/components/layout/dither-renderer.ts`; modify `DitherBackground.svelte`, `AppBackground.svelte`, `scripts/test_visual_rendering.mjs`, `scripts/test_appearance.mjs`; reuse `dither-gl.ts` shader strings unchanged unless native evidence identifies a shader defect.

**Interfaces:** `createDitherRenderer(canvas: HTMLCanvasElement, onDiagnostic: (event: DitherDiagnostic) => void): DitherRenderer | null`; `DitherRenderer { resize(width: number, height: number): void; draw(frame: DitherFrame): boolean; dispose(): void }`; `DitherFrame { time: number; settings: DitherSettings; accent: string; mouse: [number, number]; ratio: number }`; `DitherDiagnostic { event: "context_lost" | "context_restored" | "framebuffer_incomplete" | "render_failed"; detail: string }`. Renderer owns GL allocation/validation/restoration; the Svelte component retains animation, visibility and pointer scheduling. Diagnostic callbacks forward bounded warnings through the existing application logger.

- [ ] **Write failing lifecycle tests** `unrelated_blur_preview_does_not_redraw_frozen_dither`, `context_restoration_rebuilds_resources_and_draws_valid_frame`, `incomplete_framebuffer_does_not_publish_invalid_frame`, and `hidden_resize_recovers_without_unbounded_frame_loop`. Use a stateful GL fake that records actual lifecycle calls and resource identities, not an always-success proxy. Assert one initial frozen draw, no extra draw for dialogBlur-only changes, resource recreation after context restoration, no second-pass publication on incomplete framebuffer, and at most one scheduled frame while frozen/hidden. Preserve persistent drawing-buffer coverage.
- [ ] **Run red:** `node --conditions=browser --test scripts/test_visual_rendering.mjs scripts/test_appearance.mjs`.
- [ ] **Implement resource lifecycle and diagnostics.** Reuse the two-pass shaders, check framebuffer completeness after allocation/resize, handle context loss with the appropriate preventDefault/recovery behavior, and tear down all listeners/resources once. Show the configured background while no valid output exists; retain valid output where possible. Compare Dither settings by meaningful values so cloning an unchanged settings object does not restart a frozen renderer. Preserve pointer interaction, pause/resume, DPR limits and reduced motion. No readback or graphics error logging in every animation frame.
- [ ] **Collect native Windows evidence before a corruption-specific patch.** Capture frames before/during/after navigation and dialog dragging; record WebView2/runtime and graphics capability information plus context/framebuffer events. Compare animated/frozen Dither, particles/no effect, and the old native snapshot path against Task 2 foreground fades. For one diagnostic sample use an explicitly requested canvas capture, outside the frame loop, and compare its pixels with the presented window. Correct canvas plus corrupt presentation localizes the defect beyond the shader. If the defect persists and requires a different renderer, stop for the focused design revision required by the spec; otherwise implement the minimal correction supported by this evidence, then repeat the same native sequence.
- [ ] **Run green** for lifecycle/appearance/visual tests and native reproduction. Keep native visual acceptance pending if only the user can supply the affected Windows environment. Do not claim GPU corruption fixed from fake-GL tests.
- [ ] **Commit:** `fix: recover dither rendering and avoid unrelated redraws`; describe any evidence-backed compositor correction concretely in its own commit.

## Task 4: Combined Review, Windows Installer and Acceptance

**Files:** Modify `docs/releases/0.2.0-windows-test.md`; amend CI test coverage only if earlier tasks added scripts. Use existing NSIS installer and matching-symbol artifact workflow.

**Interfaces:** No new product interface. PR #183's final head identifies the installer and PDB; `build-info.txt` and SHA256 sums accompany them.

- [ ] **Run required verification:** frontend check/lint/build and all CI frontend test scripts; Rust format, Clippy with `--all-targets --all-features -- -D warnings`, all-features tests, and existing release-profile artwork command-future budget checks. Any new native scheduling/publication tests run in Windows CI as well as Linux. Record actual output and intentional external-test skips.
- [ ] **Perform independent whole-branch review** against both plans and the approved spec. Resolve actionable findings before final verification. Compare changed files with the planned scope; retain no temporary reproduction flags or default-on diagnostics. Do not merge the PR.
- [ ] **Update Windows acceptance instructions** with rapid A/B/A game opening under queued artwork, three/five-day stale-cache refresh while offline/online, repeated live blur dragging during dialog lifecycle, Dither animated/frozen with opacity 15%/blur 6 px, sidebar expanded/collapsed, repeated page transitions, resize/fullscreen and reduced motion. Document phase measurements from the artwork plan and any unresolved native graphics evidence. Require during-animation frame checks, not only settled screenshots.
- [ ] **Commit, push the updated draft PR and inspect CI results.** Produce the Windows test installer and matching PDB through the existing workflow. Verify artifact revision and installer checksum, make the installer available to the user, and state which native acceptance checks remain for them. If copying into the shared VM folder, request the normal filesystem escalation rather than changing paths or weakening permissions.
- [ ] **Wait for the user's Windows acceptance.** Keep PR draft and TaxLauncher migration pending; release merge is a subsequent explicitly authorized action after checks and acceptance.

## Self-Review and Handoff

Coverage: dialog filter ownership/lifecycle -> Task 1; stable sidebar/background/page animation/backdrop boundaries -> Task 2; Dither lifecycle, unrelated redraws and measured corruption investigation -> Task 3; independent review, checks, installer and native acceptance -> Task 4. Every Review Focus condition has a named test or an explicitly native evidence step. The Dither correction is deliberately evidence-gated rather than a guessed shader rewrite.

The user approved both plans and inline execution with one independent whole-branch review. Recommend native execution with one independent whole-branch review because the foreground targeting and graphics evidence require continuity of context. Subagent-driven execution provides separate implementation/review gates per task at higher context cost.
