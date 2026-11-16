import assert from "node:assert/strict";
import { test } from "node:test";
import { readFile } from "node:fs/promises";
import { JSDOM } from "jsdom";
import { createModuleLoader, dataModule } from "./test_module_loader.mjs";

const dom = new JSDOM("<!doctype html><html><body></body></html>", { pretendToBeVisual: true });
for (const key of ["window", "document", "navigator", "Node", "Text", "Comment", "Element", "HTMLElement", "HTMLMediaElement", "Event", "KeyboardEvent", "MouseEvent", "CustomEvent", "getComputedStyle"]) {
  Object.defineProperty(globalThis, key, { configurable: true, value: dom.window[key] });
}
HTMLElement.prototype.showPopover = function () {};
const { mount, unmount, flushSync } = await import("svelte");
const { createClassComponent } = await import("svelte/legacy");
const english = JSON.parse(await readFile(new URL("../src/lib/i18n/en.json", import.meta.url), "utf8"));
const { load } = createModuleLoader({
  "src/lib/utils/motion": dataModule("export const reducedMotion = true;"),
  "src/lib/features/library/SteamArtwork.svelte": dataModule("export default function () {}"),
  "src/lib/i18n": dataModule(`import { writable } from ${JSON.stringify(import.meta.resolve("svelte/store"))}; export const language = writable("en"); const english = ${JSON.stringify(english)}; export const t = (value, _, values = []) => (english[value] ?? value).replace(/\\{(\\d+)\\}/g, (_, index) => String(values[Number(index)] ?? ""));`),
  "src/lib/i18n/en.json": dataModule(`export default ${JSON.stringify(english)}`),
});

const settle = async () => { for (let i = 0; i < 4; i++) { await new Promise(setImmediate); flushSync(); } };
const options = [{ id: "a", name: "Portal", subtitle: "1.0" }];

test("removing the chosen link resets selection even while the link choice is hidden", async () => {
  const component = (await import(await load("src/lib/features/library/ReleasePicker.svelte"))).default;
  const target = document.createElement("div"); document.body.append(target);
  const links = ["https://gofile.io/d/one", "https://pixeldrain.com/u/two"];
  const instance = createClassComponent({ component, target, props: { steamAppId: null, options, selectedId: "a", onSelect() {}, links, link: links[1] } });
  try {
    await settle();
    assert.match(target.querySelector("button").textContent, /pixeldrain\.com/);
    instance.$set({ links: [links[0]] }); await settle();
    assert.doesNotMatch(target.querySelector("button").textContent, /pixeldrain\.com/);
    instance.$set({ links }); await settle();
    target.querySelector("button").click(); await settle();
    assert.equal(target.querySelector('[role="radio"][aria-checked="true"]').textContent.trim(), "Automatic");
  } finally { instance.$destroy(); target.remove(); }
});

test("link choices need several links and show translated unnumbered options", async () => {
  const Component = (await import(await load("src/lib/features/library/ReleasePicker.svelte"))).default;
  for (const links of [[], ["https://gofile.io/d/one"], ["https://gofile.io/d/one", "https://pixeldrain.com/u/two", "magnet:?xt=urn:btih:abc"]]) {
    const target = document.createElement("div"); document.body.append(target);
    const instance = mount(Component, { target, props: { steamAppId: null, options, selectedId: "a", onSelect() {}, links, link: "auto" } });
    try {
      await settle();
      const trigger = target.querySelector("button");
      assert.equal(trigger.disabled, true === (links.length < 2));
      if (links.length < 2) continue;
      trigger.click(); await settle();
      assert.match(target.textContent, /Download link/);
      assert.deepEqual([...target.querySelectorAll('[role="radio"]')].map(node => node.textContent.trim()), ["Automatic", "gofile.io", "pixeldrain.com", "Torrent"]);
      target.querySelectorAll('[role="radio"]')[1].click(); await settle();
      assert.match(trigger.textContent, /gofile\.io/);
      assert.equal(target.querySelector('[role="radiogroup"]'), null);
    } finally { await unmount(instance); target.remove(); }
  }
});

test("release options show source, verification and size and move with the arrow keys", async () => {
  const Component = (await import(await load("src/lib/features/library/ReleasePicker.svelte"))).default;
  const target = document.createElement("div"); document.body.append(target);
  const chosen = [];
  const releases = [
    { id: "a", name: "Portal", subtitle: "Version 1", sourceName: "SteamGG", availability: "verified", sizeBytes: 3 * 1024 ** 3 },
    { id: "b", name: "Portal", subtitle: "Version 2", sourceName: "Other", availability: "unverified", sizeBytes: null },
  ];
  const instance = mount(Component, { target, props: { steamAppId: null, options: releases, selectedId: "a", onSelect: id => chosen.push(id), links: [], link: "auto" } });
  try {
    await settle();
    const trigger = target.querySelector("button");
    assert.match(trigger.textContent, /SteamGG/);
    assert.match(trigger.textContent, /Verified/);
    trigger.click(); await settle();
    const rows = [...target.querySelectorAll('[role="option"]')];
    assert.equal(rows.length, 2);
    assert.match(rows[0].textContent, /3\.0 GB/);
    assert.match(rows[1].textContent, /Unverified/);
    assert.match(rows[1].textContent, /Unknown size/);
    assert.equal(document.activeElement, rows[0]);
    document.activeElement.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowDown", bubbles: true }));
    assert.equal(document.activeElement, rows[1]);
    document.activeElement.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
    await settle();
    assert.equal(target.querySelector('[role="listbox"]'), null);
    assert.equal(document.activeElement, trigger);
    trigger.click(); await settle();
    target.querySelectorAll('[role="option"]')[1].click(); await settle();
    assert.deepEqual(chosen, ["b"]);
  } finally { await unmount(instance); target.remove(); }
});

test("space warnings show the numbers and only enough space skips the confirmation", async () => {
  const { spaceWarning } = await import(await load("src/lib/features/downloads/space-forecast.ts"));
  const volume = { path: "/games", freeBytes: 30 * 1024 ** 3, reservedBytes: 0, minimumBytes: 20 * 1024 ** 3, estimatedBytes: 40 * 1024 ** 3 };
  assert.equal(spaceWarning({ verdict: "enough", sizeBytes: 1, volumes: [volume] }, "en"), null);
  const tight = spaceWarning({ verdict: "tight", sizeBytes: 1, volumes: [volume] }, "en");
  assert.equal(tight.tone, "primary");
  assert.equal(tight.confirmLabel, "Download anyway");
  assert.deepEqual(tight.details, ["/games: needs 20.0 GB to 40.0 GB, 30.0 GB free"]);
  const blocked = spaceWarning({ verdict: "insufficient", sizeBytes: 1, volumes: [{ ...volume, reservedBytes: 1024 ** 3 }] }, "en");
  assert.equal(blocked.tone, "danger");
  assert.equal(blocked.confirmLabel, "Ignore the warning and download");
  assert.match(blocked.details[0], /1\.0 GB already reserved/);
  const unknown = spaceWarning({ verdict: "unknown", sizeBytes: null, volumes: [{ ...volume, freeBytes: null }] }, "en");
  assert.deepEqual(unknown.details, ["The download size is unknown.", "/games: free space unavailable"]);
});

test("multipart jobs show which part is downloading", async () => {
  const { phaseLabel } = await import(await load("src/lib/features/downloads/downloads-model.ts"));
  const job = { status: "downloading", sizeBytes: 100, downloadedBytes: 40, parts: { current: 2, total: 5 }, pauseReason: null };
  assert.equal(phaseLabel(job, "en"), "Download 40% · part 2 of 5");
  assert.equal(phaseLabel({ ...job, parts: null }, "en"), "Download 40%");
  assert.equal(phaseLabel({ ...job, parts: { current: 1, total: 1 } }, "en"), "Download 40%");
  assert.doesNotMatch(phaseLabel({ ...job, status: "staging" }, "en") ?? "", /part/);
});
