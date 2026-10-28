const gap = 8;

// Opens below the trigger unless the list does not fit there and the space above is larger.
export function placePopover(
  trigger: { top: number; bottom: number },
  contentHeight: number,
  viewportHeight: number,
  limit = 256,
): { top: number; maxHeight: number } {
  const spaceAbove = Math.max(0, trigger.top - gap);
  const spaceBelow = Math.max(0, viewportHeight - trigger.bottom - gap);
  const desiredHeight = Math.min(limit, contentHeight);
  const above = spaceBelow < desiredHeight && spaceAbove > spaceBelow;
  const maxHeight = Math.floor(Math.min(desiredHeight, above ? spaceAbove : spaceBelow));
  return { top: above ? trigger.top - gap - maxHeight : trigger.bottom + gap, maxHeight };
}
