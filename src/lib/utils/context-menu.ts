const nativeMenuTargets = 'input, textarea, [contenteditable]:not([contenteditable="false"]), [data-selectable]';

export function allowsNativeContextMenu(target: EventTarget | null): boolean {
  return target instanceof Element && target.closest(nativeMenuTargets) !== null;
}
