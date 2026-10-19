import assert from "node:assert/strict";
import { test } from "node:test";
import { readFile } from "node:fs/promises";
import { JSDOM } from "jsdom";
import { createModuleLoader, dataModule } from "./test_module_loader.mjs";

const dom = new JSDOM("<!doctype html><html><body></body></html>", { pretendToBeVisual: true });
for (const key of ["window", "document", "navigator", "Node", "Text", "Comment", "Element", "HTMLElement", "HTMLMediaElement", "Event", "KeyboardEvent", "MouseEvent"]) {
  Object.defineProperty(globalThis, key, { configurable: true, value: dom.window[key] });
}
HTMLElement.prototype.showPopover = function () {};
const { mount, unmount, flushSync } = await import("svelte");
const { createClassComponent } = await import("svelte/legacy");
const english = JSON.parse(await readFile(new URL("../src/lib/i18n/en.json", import.meta.url), "utf8"));
const { load } = createModuleLoader({
  "src/lib/i18n": dataModule(`import { writable } from ${JSON.stringify(import.meta.resolve("svelte/store"))}; export const language = writable("en"); const english = ${JSON.stringify(english)}; export const t = value => english[value] ?? value;`),
  "src/lib/i18n/en.json": dataModule(`export default ${JSON.stringify(english)}`),
});

test("removing the chosen mirror resets selection even while the selector is hidden", async () => {
  const component = (await import(await load("src/lib/features/library/DownloadMirrorSelect.svelte"))).default;
  const target = document.createElement("div"); document.body.append(target);
  const uris = ["https://gofile.io/d/one", "https://pixeldrain.com/u/two"];
  const instance = createClassComponent({ component, target, props: { uris, value: uris[1] } });
  try {
    await settle();
    assert.equal(target.querySelector("button").textContent.trim(), "pixeldrain.com");
    instance.$set({ uris: [uris[0]] }); await settle();
    assert.equal(target.querySelector("button"), null);
    instance.$set({ uris }); await settle();
    assert.equal(target.querySelector("button").textContent.trim(), "Automatic");
  } finally { instance.$destroy(); target.remove(); }
});
const settle = async () => { for (let i = 0; i < 4; i++) { await new Promise(setImmediate); flushSync(); } };

test("download mirror selector is hidden without a choice and shows translated unnumbered options", async () => {
  const Component = (await import(await load("src/lib/features/library/DownloadMirrorSelect.svelte"))).default;
  for (const uris of [[], ["https://gofile.io/d/one"], ["https://gofile.io/d/one", "https://pixeldrain.com/u/two"]]) {
    const target = document.createElement("div"); document.body.append(target);
    const instance = mount(Component, { target, props: { uris, value: "auto" } });
    try {
      await settle();
      const trigger = target.querySelector("button");
      assert.equal(trigger !== null, uris.length > 1);
      if (!trigger) continue;
      assert.match(target.textContent, /Download link/);
      assert.match(trigger.textContent, /Automatic/);
      trigger.click(); await settle();
      assert.deepEqual([...target.querySelectorAll('[role="option"]')].map(node => node.textContent.trim()), ["Automatic", "gofile.io", "pixeldrain.com"]);
      target.querySelectorAll('[role="option"]')[1].click(); await settle();
      assert.equal(trigger.textContent.trim(), "gofile.io");
    } finally { await unmount(instance); target.remove(); }
  }
});
