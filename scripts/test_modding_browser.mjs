import assert from "node:assert/strict";
import { test } from "node:test";
import { JSDOM } from "jsdom";
import { createModuleLoader, dataModule } from "./test_module_loader.mjs";

const dom = new JSDOM("<!doctype html><html><body></body></html>", { pretendToBeVisual: true });
for (const key of [
  "window", "document", "navigator", "Node", "Text", "Comment", "Element", "HTMLElement", "HTMLInputElement",
  "HTMLButtonElement", "HTMLMediaElement", "Event", "KeyboardEvent", "MouseEvent", "MutationObserver",
]) {
  Object.defineProperty(globalThis, key, { configurable: true, value: dom.window[key] });
}
globalThis.matchMedia = () => ({ matches: false });
const { mount, unmount, flushSync } = await import("svelte");
const settle = async () => { for (let i = 0; i < 6; i++) { await new Promise((resolve) => setImmediate(resolve)); flushSync(); } };

const storeImport = `import { writable } from "${import.meta.resolve("svelte/store")}";`;
const { load } = createModuleLoader({
  "src/lib/i18n": dataModule(storeImport + 'export const language=writable("it"); export const t=(m,l,v=[])=>m.replace(/\\{(\\d+)\\}/g,(_,i)=>String(v[i]??""));'),
  "src/lib/services/invoke": dataModule("export const invoke=(...args)=>globalThis.browserInvoke(...args);"),
  "@tauri-apps/api/event": dataModule("export const listen=async()=>()=>{};"),
});
const model = await import(await load("src/lib/features/modding/modding-model.ts"));

test("category chips cycle through include, exclude and neutral", () => {
  let filters = model.defaultBrowserFilters;
  assert.equal(model.categoryState(filters, "Tools"), "neutral");
  filters = model.cycleCategory(filters, "Tools");
  assert.deepEqual([filters.categories, filters.excludedCategories], [["Tools"], []]);
  assert.equal(model.categoryState(filters, "Tools"), "included");
  filters = model.cycleCategory(model.cycleCategory(filters, "Mods"), "Mods");
  assert.deepEqual([filters.categories, filters.excludedCategories], [["Tools"], ["Mods"]]);
  filters = model.cycleCategory(filters, "Mods");
  assert.deepEqual([filters.categories, filters.excludedCategories], [["Tools"], []]);
  assert.deepEqual(model.defaultBrowserFilters.categories, [], "the defaults are never mutated");
});

test("filter queries carry the category mode and exclusions only when used", () => {
  const base = model.defaultBrowserFilters;
  const plain = model.packageQuery(base, 0);
  assert.equal("categories" in plain || "categoryMode" in plain || "excludedCategories" in plain, false);
  const filters = { ...base, categories: ["A", "B"], categoryMode: "all", excludedCategories: ["C"], includeNsfw: true };
  const query = model.packageQuery(filters, 30);
  assert.deepEqual([query.categories, query.categoryMode, query.excludedCategories, query.includeNsfw, query.offset], [["A", "B"], "all", ["C"], true, 30]);
  assert.equal(model.packageQuery({ ...base, excludedCategories: ["C"] }, 0).categories, undefined);
});

test("the active filter count and the reset keep the search and sort", () => {
  const base = model.defaultBrowserFilters;
  assert.equal(model.activeFilterCount(base), 0);
  const filters = { ...base, text: "suits", sort: "topRated", categories: ["A"], categoryMode: "all", excludedCategories: ["B", "C"], includeDeprecated: true, includeNsfw: true };
  assert.equal(model.activeFilterCount(filters), 5);
  const cleared = model.clearFilterOptions(filters);
  assert.equal(model.activeFilterCount(cleared), 0);
  assert.deepEqual([cleared.text, cleared.sort, cleared.categoryMode], ["suits", "topRated", "any"]);
});

test("the document view opens links outside the webview", async () => {
  const calls = [];
  globalThis.browserInvoke = async (command, args) => { calls.push([command, args]); if (args.url.includes("fail")) throw new Error("no browser"); };
  const Component = (await import(await load("src/lib/features/modding/ModDocument.svelte"))).default;
  const target = document.createElement("div");
  document.body.append(target);
  const errors = [];
  const instance = mount(Component, {
    target,
    props: { html: '<p>See <a href="https://example.com/a">site</a> and <a href="https://example.com/fail">bad</a> <strong>x</strong></p>', onError: (message) => errors.push(message) },
  });
  await settle();
  try {
    const click = (element) => {
      const event = new MouseEvent("click", { bubbles: true, cancelable: true });
      element.dispatchEvent(event);
      return event.defaultPrevented;
    };
    const [site, bad] = target.querySelectorAll("a");
    assert.equal(click(site), true, "navigation is cancelled");
    await settle();
    assert.deepEqual(calls, [["open_mod_external_link", { url: "https://example.com/a" }]]);
    assert.equal(click(target.querySelector("strong")), false, "other clicks are untouched");
    click(bad);
    await settle();
    assert.deepEqual(errors, ["no browser"]);
  } finally {
    await unmount(instance);
    target.remove();
    delete globalThis.browserInvoke;
  }
});
