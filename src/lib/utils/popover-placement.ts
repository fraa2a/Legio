const margin = 8;

// The list grows out of the trigger edge: below unless it does not fit there and the space above is larger.
export function placePopover(
  trigger: { top: number; bottom: number },
  contentHeight: number,
  viewportHeight: number,
  limit = 256,
): { above: boolean; maxHeight: number } {
  const spaceAbove = Math.max(0, trigger.top - margin);
  const spaceBelow = Math.max(0, viewportHeight - trigger.bottom - margin);
  const desiredHeight = Math.min(limit, contentHeight);
  const above = spaceBelow < desiredHeight && spaceAbove > spaceBelow;
  return { above, maxHeight: Math.floor(Math.min(desiredHeight, above ? spaceAbove : spaceBelow)) };
}
