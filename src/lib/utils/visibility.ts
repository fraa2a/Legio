export function observeVisibility(
  node: HTMLElement | undefined,
  onVisible: (visible: boolean) => void,
  once = true,
): () => void {
  if (node === undefined || typeof IntersectionObserver === "undefined") {
    onVisible(true);
    return () => undefined;
  }
  const observer = new IntersectionObserver((entries) => {
    const visible = entries.some(entry => entry.isIntersecting);
    if (once && !visible) return;
    onVisible(visible);
    if (once) observer.disconnect();
  });
  observer.observe(node);
  return () => observer.disconnect();
}
