export function observeVisibility(
  node: HTMLElement | undefined,
  onVisible: () => void,
): () => void {
  if (node === undefined || typeof IntersectionObserver === "undefined") {
    onVisible();
    return () => undefined;
  }
  const observer = new IntersectionObserver((entries) => {
    if (!entries.some((entry) => entry.isIntersecting)) return;
    // One-shot: tiles only need to be observed until they enter the viewport.
    onVisible();
    observer.disconnect();
  });
  observer.observe(node);
  return () => observer.disconnect();
}
