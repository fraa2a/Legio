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
  const animations = collectForegroundTargets(root).map(node => {
    const opacity = Number.parseFloat(getComputedStyle(node).opacity);
    const visibleOpacity = Number.isFinite(opacity) ? opacity : 1;
    const originalOpacity = node.style.getPropertyValue("opacity");
    const originalPriority = node.style.getPropertyPriority("opacity");
    const endOpacity = direction === "in" ? visibleOpacity : 0;
    const frames = direction === "in" ? [{ opacity: 0 }, { opacity: visibleOpacity }] : [{ opacity: visibleOpacity }, { opacity: 0 }];
    // Skia can expose the underlying style when the compositor animation finishes.
    node.style.setProperty("opacity", String(endOpacity));
    const animation = node.animate(frames, { duration, easing: "linear", fill: "both" });
    return { node, animation, originalOpacity, originalPriority };
  });
  let cancelled = false;
  const finished = Promise.all(animations.map(({ animation }) => animation.finished.catch(error => {
    if (!cancelled) console.warn("Page foreground animation failed", error);
  }))).then(() => undefined);
  return { finished, cancel() {
    if (cancelled) return;
    cancelled = true;
    for (const { node, animation, originalOpacity, originalPriority } of animations) {
      animation.cancel();
      // Retired native layers may outlive their detached DOM nodes for one frame.
      if (direction === "out" && !node.isConnected) continue;
      if (originalOpacity === "") node.style.removeProperty("opacity");
      else node.style.setProperty("opacity", originalOpacity, originalPriority);
    }
  } };
}
