const allowedTags = new Set([
  "a", "b", "blockquote", "br", "code", "div", "em", "h1", "h2", "h3", "h4",
  "hr", "i", "li", "ol", "p", "pre", "span", "strong", "ul",
]);
const ignoredTags = new Set(["form", "iframe", "img", "input", "math", "object", "script", "style", "svg"]);

export function renderSteamDescription(target: HTMLElement, source: string): { update: (value: string) => void } {
  function render(value: string): void {
    const parsed = new DOMParser().parseFromString(value, "text/html");
    const output = document.createDocumentFragment();

    function appendSafe(parent: Node, node: Node): void {
      if (node.nodeType === Node.TEXT_NODE) {
        parent.appendChild(document.createTextNode(node.textContent ?? ""));
        return;
      }
      if (node.nodeType !== Node.ELEMENT_NODE) return;
      const element = node as Element;
      const tag = element.tagName.toLowerCase();
      if (ignoredTags.has(tag)) return;
      const safe = allowedTags.has(tag) ? document.createElement(tag) : parent;
      if (safe !== parent) parent.appendChild(safe);
      if (tag === "a" && safe instanceof HTMLAnchorElement) {
        const href = element.getAttribute("href");
        if (href !== null) {
          try {
            const url = new URL(href);
            if (url.protocol === "https:" || url.protocol === "http:") {
              safe.href = url.href;
              safe.target = "_blank";
              safe.rel = "noopener noreferrer";
            }
          } catch {
            // Invalid links keep their visible text.
          }
        }
      }
      for (const child of element.childNodes) appendSafe(safe, child);
    }

    for (const child of parsed.body.childNodes) appendSafe(output, child);
    target.replaceChildren(output);
  }

  render(source);
  return { update: render };
}

export function steamSummaryLine(source: string | null): string | null {
  if (source === null) return null;
  const text = source
    .replace(/<[^>]*>/g, " ")
    .replace(/&nbsp;/gi, " ")
    .replace(/&quot;/gi, '"')
    .replace(/&#0?39;/g, "'")
    .replace(/&lt;/gi, "<")
    .replace(/&gt;/gi, ">")
    .replace(/&amp;/gi, "&")
    .replace(/\s+/g, " ")
    .trim();
  return text.length === 0 ? null : text;
}
