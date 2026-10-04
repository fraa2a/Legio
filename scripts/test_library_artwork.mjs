import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { test } from "node:test";
import { compile } from "svelte/compiler";
import ts from "typescript";
import { JSDOM } from "jsdom";

const dom = new JSDOM("<!doctype html><html><body></body></html>", { pretendToBeVisual: true });
for (const key of ["window", "document", "navigator", "Node", "Text", "Comment", "Element", "HTMLElement", "Event", "CustomEvent", "getComputedStyle", "requestAnimationFrame", "cancelAnimationFrame"]) {
  const value = dom.window[key];
  Object.defineProperty(globalThis, key, { configurable: true, value: typeof value === "function" && key.endsWith("AnimationFrame") ? value.bind(dom.window) : value });
}
const { mount, unmount, flushSync } = await import("svelte");
const dataModule = (code) => `data:text/javascript;base64,${Buffer.from(code).toString("base64")}`;
const mocks = {
  "@tauri-apps/api/core": dataModule("export const invoke = (...args) => globalThis.artworkInvoke(...args);"),
  "src/lib/i18n": dataModule(`import { writable } from "${import.meta.resolve("svelte/store")}"; export const language = writable("en"); export const t = value => value;`),
  "src/lib/utils/motion": dataModule("export const fadeDuration = 0;"),
};
const modules = new Map();
async function moduleUrl(path) {
  if (mocks[path]) return mocks[path];
  if (modules.has(path)) return modules.get(path);
  const loading = (async () => {
    const source = await readFile(new URL(`../${path}`, import.meta.url), "utf8");
    const code = path.endsWith(".svelte")
      ? compile(source, { filename: path, generate: "client" }).js.code
      : ts.transpileModule(source, { compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ESNext } }).outputText;
    return dataModule(await resolveImports(code, path));
  })();
  modules.set(path, loading);
  return loading;
}
async function resolveImports(code, path) {
  for (const match of [...code.matchAll(/(?:from|import)\s*["']([^"']+)["']/g)]) {
    const specifier = match[1];
    let url;
    if (specifier.startsWith(".")) {
      const resolved = new URL(specifier, `file:///${path}`).pathname.slice(1);
      url = await moduleUrl(resolved.endsWith(".svelte") ? resolved : resolved + (mocks[resolved] ? "" : ".ts"));
    } else {
      url = mocks[specifier] ?? import.meta.resolve(specifier);
    }
    code = code.replaceAll(`"${specifier}"`, JSON.stringify(url)).replaceAll(`'${specifier}'`, JSON.stringify(url));
  }
  return code;
}
const requests = [];
let detailsResolve;
globalThis.artworkInvoke = (command, args) => {
  requests.push([command, args]);
  if (command === "get_steam_details") return new Promise((resolve) => { detailsResolve = resolve; });
  assert.equal(command, "get_steam_asset");
  return Promise.resolve({ bytes: [1], contentType: "image/png", stale: false, cacheWarning: null });
};
const settle = async () => {
  for (let i = 0; i < 8; i++) { await new Promise((resolve) => setImmediate(resolve)); flushSync(); }
};

// Each App ID starts with empty details and image caches.
test("visible library cover loads while details are pending and remains after an empty response", async () => {
  requests.length = 0;
  let intersect;
  globalThis.IntersectionObserver = class {
    constructor(callback) { intersect = callback; }
    observe() {}
    disconnect() {}
  };
  const source = `<script>import ArtworkTile from "./src/lib/components/ui/ArtworkTile.svelte";</script><ul><ArtworkTile steamAppId={400} monogram="P">Portal</ArtworkTile></ul>`;
  const url = dataModule(await resolveImports(compile(source, { generate: "client" }).js.code, "fixture.svelte"));
  const target = document.createElement("div");
  document.body.append(target);
  const component = mount((await import(url)).default, { target });
  try {
    await settle();
    assert.equal(requests.length, 0, "offscreen covers must remain lazy");
    intersect([{ isIntersecting: true }]);
    await settle();
    assert.ok(requests.some(([command]) => command === "get_steam_details"));
    assert.ok(requests.some(([command, args]) => command === "get_steam_asset" && args.steamAppId === 400 && args.asset === "hero_blur"));
    assert.match(target.querySelector("img")?.src ?? "", /^blob:/);
    detailsResolve({ details: null, cachedAt: null, stale: false });
    await settle();
    assert.match(target.querySelector("img")?.src ?? "", /^blob:/);
  } finally {
    await unmount(component);
    target.remove();
    delete globalThis.IntersectionObserver;
  }
});

test("standalone game logo loads without opening game details", async () => {
  requests.length = 0;
  const { default: GameLogo } = await import(await moduleUrl("src/lib/features/library/GameLogo.svelte"));
  const target = document.createElement("div");
  document.body.append(target);
  const component = mount(GameLogo, { target, props: { game: { id: "portal", name: "Portal", steamAppId: 401 } } });
  try {
    await settle();
    assert.ok(requests.some(([command, args]) => command === "get_steam_asset" && args.steamAppId === 401 && args.asset === "logo"));
    assert.match(target.querySelector("img")?.src ?? "", /^blob:/);
    assert.equal(requests.filter(([command]) => command === "get_steam_details").length, 0);
  } finally {
    await unmount(component);
    target.remove();
  }
});

test("manual game keeps its name without requesting Steam artwork", async () => {
  requests.length = 0;
  const { default: GameLogo } = await import(await moduleUrl("src/lib/features/library/GameLogo.svelte"));
  const target = document.createElement("div");
  const component = mount(GameLogo, { target, props: { game: { id: "manual", name: "Manual game", steamAppId: null } } });
  try {
    await settle();
    assert.equal(target.textContent, "Manual game");
    assert.equal(requests.length, 0);
  } finally {
    await unmount(component);
  }
});

test("steam summary line strips markup, decodes entities once and collapses whitespace", async () => {
  const { steamSummaryLine } = await import(await moduleUrl("src/lib/features/library/steam-description.ts"));
  assert.equal(steamSummaryLine(null), null);
  assert.equal(steamSummaryLine("   "), null);
  assert.equal(steamSummaryLine("<p><br/></p>"), null);
  assert.equal(steamSummaryLine("<p>Un&#039;avventura &quot;spaziale&quot;.</p>"), `Un'avventura "spaziale".`);
  assert.equal(steamSummaryLine("Fate &lt;3 &amp; fate&nbsp;cosi"), "Fate <3 & fate cosi");
  assert.equal(steamSummaryLine("&amp;lt;testo&amp;gt;"), "&lt;testo&gt;");
  assert.equal(steamSummaryLine("riga\n\n  due\tspazi"), "riga due spazi");
});

test("store card keeps its footprint, loads a blurred background and shows the Steam blurb", async () => {
  requests.length = 0;
  let intersect;
  globalThis.IntersectionObserver = class {
    constructor(callback) { intersect = callback; }
    observe() {}
    disconnect() {}
  };
  const { default: StoreGameCard } = await import(await moduleUrl("src/lib/features/store/StoreGameCard.svelte"));
  const target = document.createElement("div");
  document.body.append(target);
  const component = mount(StoreGameCard, {
    target,
    props: {
      steamAppId: 620,
      name: "Portal 2",
      status: {
        availability: "verified",
        entry: {
          steamAppId: 620,
          name: "Portal 2",
          release: { version: "1.0.0", publishedAt: "2020-01-01" },
          download: { url: "https://example.test/portal2.zip", sizeBytes: 3 * 1024 * 1024 * 1024 },
        },
      },
      onOpen: () => {},
    },
  });
  try {
    await settle();
    assert.equal(requests.length, 0, "offscreen store cards must remain lazy");

    const cover = target.querySelector("button > div");
    for (const token of ["w-56", "sm:w-72"]) {
      assert.ok(cover.className.split(" ").includes(token), `the cover keeps ${token}`);
    }
    assert.ok(!cover.className.includes("rounded"), "the sharp cover reaches the card edges");
    const button = target.querySelector("button");
    for (const token of ["items-stretch", "min-h-28", "sm:min-h-32"]) {
      assert.ok(button.className.split(" ").includes(token), `the card keeps ${token}`);
    }
    assert.ok(!button.className.includes("p-2"), "the sharp cover keeps the horizontal space");
    const card = target.querySelector("li");
    assert.ok(card.className.includes("overflow-hidden") && card.className.includes("rounded-xl"), "the card clips the flush cover");
    assert.ok(!card.className.includes("border"), "the card has no border");
    assert.equal(target.querySelectorAll("li > div[aria-hidden]").length, 2, "gradient base and legibility scrim are always mounted");

    intersect([{ isIntersecting: true }]);
    await settle();
    assert.ok(requests.some(([command, args]) => command === "get_steam_details" && args.steamAppId === 620));
    assert.ok(requests.some(([command, args]) => command === "get_steam_asset" && args.steamAppId === 620 && args.asset === "hero_blur"));
    const images = [...target.querySelectorAll("img")];
    assert.equal(images.length, 2, "background and thumbnail both render artwork");
    for (const image of images) assert.match(image.src, /^blob:/);
    assert.ok(cover.querySelector("img").className.includes("object-contain"), "the sharp cover is never cropped");

    const blurb = [...target.querySelectorAll("span")].find((node) => node.className.split(" ").includes("h-4"));
    assert.ok(blurb, "the blurb row is reserved while details load");
    assert.equal(blurb.textContent, "", "the reserved row must not push the layout when it is empty");
    assert.match(target.textContent, /1\.0\.0/);
    assert.match(target.textContent, /GB/);

    detailsResolve({
      details: {
        steamAppId: 620,
        name: "Portal 2",
        appType: "game",
        shortDescription: "<p>Un&#039;avventura &quot;spaziale&quot;.</p>",
        detailedDescription: null,
        systemRequirements: null,
        developers: [],
        publishers: [],
        genres: [],
        platforms: null,
        releaseDate: null,
        assets: { header: null, capsule: null, background: null, screenshots: [] },
      },
      cachedAt: 1,
      stale: false,
    });
    await settle();
    assert.equal(blurb.textContent, `Un'avventura "spaziale".`);
  } finally {
    await unmount(component);
    target.remove();
    delete globalThis.IntersectionObserver;
  }
});

test("store badge reports download availability and drops the trust states", async () => {
  const { sourceBadgeFor } = await import(await moduleUrl("src/lib/features/store/source-status.ts"));
  for (const availability of ["verified", "unverified"]) {
    assert.deepEqual(sourceBadgeFor(availability), { label: "Available", tone: "info" }, `${availability} has a download source`);
  }
  for (const availability of ["unavailable", "unknown"]) {
    assert.deepEqual(sourceBadgeFor(availability), { label: "Unavailable", tone: "danger" }, `${availability} has no download source`);
  }
});
