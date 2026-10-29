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
    let protectedSubtree = node.matches(".legio-glass,.legio-panel,.legio-popover,.legio-dialog-surface")
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
  if (duration <= 0) return { finished: Promise.resolve(), cancel() {} };
  const targets = collectForegroundTargets(root).map(node => {
    const opacity = Number.parseFloat(getComputedStyle(node).opacity);
    return { node, opacity: Number.isFinite(opacity) ? opacity : 1,
      original: node.style.getPropertyValue("opacity"), priority: node.style.getPropertyPriority("opacity") };
  });
  if (targets.length === 0) return { finished: Promise.resolve(), cancel() {} };
  let resolveFinished = () => {};
  const finished = new Promise<void>(resolve => { resolveFinished = resolve; });
  let frame: number | undefined;
  let start: number | undefined;
  let cancelled = false;
  function paint(progress: number): void {
    const factor = direction === "in" ? progress : 1 - progress;
    for (const target of targets) target.node.style.setProperty("opacity", String(target.opacity * factor), "important");
  }
  function tick(time: number): void {
    frame = undefined;
    if (cancelled) return;
    start ??= time;
    const progress = Math.min(1, Math.max(0, (time - start) / duration));
    paint(progress);
    if (progress === 1) resolveFinished();
    else frame = requestAnimationFrame(tick);
  }
  paint(0);
  frame = requestAnimationFrame(tick);
  return { finished, cancel() {
    if (cancelled) return;
    cancelled = true;
    if (frame !== undefined) cancelAnimationFrame(frame);
    for (const target of targets) {
      if (direction === "out" && !target.node.isConnected) continue;
      if (target.original === "") target.node.style.removeProperty("opacity");
      else target.node.style.setProperty("opacity", target.original, target.priority);
    }
    resolveFinished();
  } };
}
