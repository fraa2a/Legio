import { createSteamVideoSurface } from "./steam-description-video";

// Links keep only their text: the webview cannot open them in the system browser.
const allowedTags = new Set([
  "b", "blockquote", "br", "code", "div", "em", "h1", "h2", "h3", "h4",
  "hr", "i", "li", "ol", "p", "pre", "span", "strong", "ul",
]);
const ignoredTags = new Set(["form", "iframe", "input", "math", "object", "script", "style", "svg"]);

function steamDescriptionMediaUrl(source: string | null): string | null {
  if (source === null) return null;
  try {
    const url = new URL(source);
    if (url.protocol !== "https:" || url.username !== "" || url.password !== "" || url.port !== "") return null;
    const steamHost = ["steamstatic.com", "steamusercontent.com", "steampowered.com"]
      .some(domain => url.hostname === domain || url.hostname.endsWith(`.${domain}`));
    const legacyHost = ["steamcdn-a.akamaihd.net", "steamuserimages-a.akamaihd.net"].includes(url.hostname);
    return steamHost || legacyHost ? url.href : null;
  } catch {
    return null;
  }
}

function steamDescriptionVideo(element: Element): HTMLVideoElement | null {
  const src = steamDescriptionMediaUrl(element.getAttribute("src"));
  const sources = [...element.children].filter(child => child.tagName.toLowerCase() === "source")
    .flatMap(child => {
      const source = steamDescriptionMediaUrl(child.getAttribute("src"));
      return source === null ? [] : [{ src: source, type: child.getAttribute("type") }];
    });
  if (src === null && sources.length === 0) return null;
  const video = document.createElement("video");
  video.muted = true;
  video.defaultMuted = true;
  video.autoplay = !matchMedia("(prefers-reduced-motion: reduce)").matches;
  video.loop = true;
  video.playsInline = true;
  video.controls = false;
  video.preload = "metadata";
  video.className = "my-3 block h-auto max-w-full rounded-lg";
  const poster = steamDescriptionMediaUrl(element.getAttribute("poster"));
  if (poster !== null) video.poster = poster;
  for (const dimension of ["width", "height"] as const) {
    const value = Number(element.getAttribute(dimension));
    if (Number.isInteger(value) && value > 0 && value <= 16384) video[dimension] = value;
  }
  if (src !== null) video.src = src;
  for (const item of sources) {
    const source = document.createElement("source");
    source.src = item.src;
    if (item.type !== null && /^video\/(webm|mp4)(?:\s*;|$)/i.test(item.type)) source.type = item.type;
    video.appendChild(source);
  }
  return video;
}

function suspendGif(image: HTMLImageElement): void {
  if (!image.complete || image.naturalWidth <= 0 || image.getAttribute("src") === null) return;
  if (!image.hasAttribute("width")) image.width = image.naturalWidth;
  if (!image.hasAttribute("height")) image.height = image.naturalHeight;
  image.dataset.steamGifSuspended = "true";
  image.removeAttribute("src");
}

export function renderSteamDescription(target: HTMLElement, source: string): { update: (value: string) => void; destroy: () => void } {
  let media: (() => void)[] = [];
  let gifObserver: IntersectionObserver | undefined;
  function dispose(): void {
    gifObserver?.disconnect();
    gifObserver = undefined;
    for (const destroy of media) destroy();
    media = [];
  }
  function render(value: string): void {
    dispose();
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
      if (tag === "video") {
        const video = steamDescriptionVideo(element);
        if (video !== null) {
          const surface = createSteamVideoSurface(video);
          media.push(surface.destroy);
          parent.appendChild(surface.element);
        }
        return;
      }
      if (tag === "img") {
        const src = steamDescriptionMediaUrl(element.getAttribute("src"));
        if (src === null) return;
        const image = document.createElement("img");
        image.alt = element.getAttribute("alt") ?? "";
        image.setAttribute("loading", "lazy");
        image.setAttribute("decoding", "async");
        image.setAttribute("referrerpolicy", "no-referrer");
        image.className = "my-3 block h-auto max-w-full rounded-lg";
        const width = Number(element.getAttribute("width"));
        const height = Number(element.getAttribute("height"));
        if (Number.isInteger(width) && width > 0 && width <= 16384) image.width = width;
        if (Number.isInteger(height) && height > 0 && height <= 16384) image.height = height;
        image.src = src;
        const isGif = new URL(src).pathname.toLowerCase().endsWith(".gif");
        if (isGif && typeof IntersectionObserver !== "undefined") {
          image.dataset.steamGifSrc = src;
          image.dataset.steamGifNear = "true";
          gifObserver ??= new IntersectionObserver(entries => {
            for (const entry of entries) {
              const target = entry.target as HTMLImageElement;
              target.dataset.steamGifNear = String(entry.isIntersecting);
              if (entry.isIntersecting) {
                if (target.dataset.steamGifSuspended === "true") {
                  delete target.dataset.steamGifSuspended;
                  target.src = target.dataset.steamGifSrc ?? "";
                }
              } else {
                suspendGif(target);
              }
            }
          }, { rootMargin: "300px 0px" });
          const suspend = () => {
            if (image.dataset.steamGifNear === "false") suspendGif(image);
          };
          image.addEventListener("load", suspend);
          media.push(() => image.removeEventListener("load", suspend));
          gifObserver.observe(image);
        }
        parent.appendChild(image);
        return;
      }
      if (ignoredTags.has(tag)) return;
      const safe = allowedTags.has(tag) ? document.createElement(tag) : parent;
      if (safe !== parent) parent.appendChild(safe);
      for (const child of element.childNodes) appendSafe(safe, child);
    }

    for (const child of parsed.body.childNodes) appendSafe(output, child);
    target.replaceChildren(output);
  }

  render(source);
  return { update: render, destroy: dispose };
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
