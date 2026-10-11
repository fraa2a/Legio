export interface ForegroundAnimation {
  finished: Promise<void>;
  cancel(): void;
}

export function collectForegroundTargets(root: HTMLElement): HTMLElement[] {
  const targets: HTMLElement[] = [];
  function visit(node: HTMLElement): boolean {
    const start = targets.length;
    const style = getComputedStyle(node);
    const backdrop = style.getPropertyValue("backdrop-filter");
    const webkit = style.getPropertyValue("-webkit-backdrop-filter");
    let protectedSubtree = node.matches(".legio-glass,.legio-panel,.legio-dialog-surface")
      || (backdrop !== "" && backdrop !== "none") || (webkit !== "" && webkit !== "none");
    for (const child of node.children) {
      if (child instanceof HTMLElement && visit(child)) protectedSubtree = true;
    }
    if (!protectedSubtree) {
      targets.splice(start);
      targets.push(node);
    }
    return protectedSubtree;
  }
  visit(root);
  return targets;
}

export function animateForeground(root: HTMLElement, direction: "in" | "out", duration: number): ForegroundAnimation {
  if (duration === 0 || typeof root.animate !== "function") return { finished: Promise.resolve(), cancel() {} };
  const frames = direction === "in" ? [{ opacity: 0 }, { opacity: 1 }] : [{ opacity: 1 }, { opacity: 0 }];
  const animations = collectForegroundTargets(root).map(node => node.animate(frames, { duration, easing: "linear", fill: "both" }));
  let cancelled = false;
  const finished = Promise.all(animations.map(animation => animation.finished.catch(error => {
    if (!cancelled) console.warn("Page foreground animation failed", error);
  }))).then(() => undefined);
  return { finished, cancel() { cancelled = true; for (const animation of animations) animation.cancel(); } };
}
