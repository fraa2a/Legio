export interface ForegroundAnimation {
  finished: Promise<void>;
  cancel(): void;
}

// Opacity on an ancestor creates a backdrop root, so these surfaces would stop blurring the wallpaper.
const backdropSurfaces = ".legio-glass,.legio-panel,.legio-popover,.legio-dialog-surface,[class*='backdrop-blur'],[style*='backdrop-filter']";

export function collectForegroundTargets(root: HTMLElement): HTMLElement[] {
  const targets: HTMLElement[] = [];
  function visit(node: HTMLElement): void {
    if (!node.matches(backdropSurfaces) && node.querySelector(backdropSurfaces) === null) {
      targets.push(node);
      return;
    }
    for (const child of node.children) if (child instanceof HTMLElement) visit(child);
  }
  visit(root);
  return targets;
}

export function animateForeground(root: HTMLElement, direction: "in" | "out", duration: number): ForegroundAnimation {
  if (duration <= 0) return { finished: Promise.resolve(), cancel() {} };
  const animations = collectForegroundTargets(root).map(node => {
    const parsed = Number.parseFloat(getComputedStyle(node).opacity);
    const opacity = Number.isFinite(parsed) ? parsed : 1;
    const keyframes = direction === "in" ? [{ opacity: 0 }, { opacity }] : [{ opacity }, { opacity: 0 }];
    const animation = node.animate(keyframes, { duration, easing: "linear", fill: "forwards" });
    // A finished fade-in must not keep overriding opacity set later by CSS (hover, state classes).
    if (direction === "in") void animation.finished.then(() => animation.cancel(), () => undefined);
    return animation;
  });
  return {
    // A cancelled animation rejects its finished promise; cancellation still ends the transition.
    finished: Promise.all(animations.map(animation => animation.finished.catch(() => undefined))).then(() => undefined),
    cancel() { for (const animation of animations) animation.cancel(); },
  };
}
