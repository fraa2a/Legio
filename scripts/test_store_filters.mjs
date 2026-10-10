import assert from "node:assert/strict";
import { test } from "node:test";
import { JSDOM } from "jsdom";
import { writable } from "svelte/store";
import { createModuleLoader, dataModule } from "./test_module_loader.mjs";

const dom = new JSDOM("<!doctype html><html><body></body></html>", { pretendToBeVisual: true });
for (const key of ["window", "document", "navigator", "Node", "Text", "Comment", "Element", "HTMLElement", "HTMLMediaElement", "Event", "KeyboardEvent", "MouseEvent"]) {
  Object.defineProperty(globalThis, key, { configurable: true, value: dom.window[key] });
}
HTMLElement.prototype.showPopover = function () {};
const { mount, unmount, flushSync } = await import("svelte");
const settle = async () => { flushSync(); await new Promise(resolve => setImmediate(resolve)); flushSync(); };
const searchDelay = async () => { await settle(); await new Promise(resolve => setTimeout(resolve, 330)); await settle(); };

test("store controls combine filters and reset pagination when either choice changes", async () => {
  const calls = [];
  const more = [];
  const notifications = [];
  globalThis.storeFilterNotifications = notifications;
  const results = Array.from({ length: 80 }, (_, index) => ({ steamAppId: index + 1, name: `Game ${index + 1}` }));
  globalThis.storeFilterFixture = {
    catalog: writable({ results, total: 120, status: "ready", refreshing: false, error: null, nextOffset: null }),
    source: writable({ data: { manifest: null, warning: null } }),
    query: writable("portal"),
    search: (...args) => calls.push(args), more: (...args) => more.push(args),
  };
  const stores = import.meta.resolve("svelte/store");
  const { load } = createModuleLoader({
    "src/lib/i18n": dataModule(`import { writable } from ${JSON.stringify(stores)}; export const language = writable("it"); export const t = value => value;`),
    "src/lib/stores/catalog": dataModule("export const catalog = globalThis.storeFilterFixture.catalog; export const runCatalogSearch = (...args) => globalThis.storeFilterFixture.search(...args); export const loadMoreCatalog = (...args) => globalThis.storeFilterFixture.more(...args);"),
    "src/lib/stores/library-ui": dataModule("export const storeQuery = globalThis.storeFilterFixture.query;"),
    "src/lib/stores/source": dataModule(`import { writable } from ${JSON.stringify(stores)}; export const source = globalThis.storeFilterFixture.source; export const sourceRefreshError = writable(null); export const refreshSource = async () => {};`),
    "src/lib/stores/navigation": dataModule(`import { writable } from ${JSON.stringify(stores)}; export const selectedStoreGame = writable(null); export const openStoreGame = () => {};`),
    "src/lib/stores/toast": dataModule("export const showToast = (...args) => globalThis.storeFilterNotifications.push(args);"),
    "src/lib/features/store/StoreGameCard.svelte": dataModule("export default function () {}"),
    "src/lib/features/library/GameDetailView.svelte": dataModule("export default function () {}"),
  });
  const StoreView = (await import(await load("src/lib/features/store/StoreView.svelte"))).default;
  const target = document.createElement("div");
  document.body.append(target);
  const instance = mount(StoreView, { target });
  const loadMore = () => [...target.querySelectorAll("button")].find(button => button.textContent.includes("Carica altri 20")).click();
  async function choose(id, label) {
    const trigger = target.querySelector(`#${id}`);
    assert.ok(trigger, `${id} must be available before a search returns results`);
    trigger.click(); await settle();
    [...target.querySelectorAll('[role="option"]')].find(option => option.textContent.trim() === label).click();
    await searchDelay();
  }
  try {
    await searchDelay();
    assert.deepEqual(calls.at(-1), ["portal", { sort: "relevance", availability: "all" }]);
    loadMore(); await settle();
    loadMore(); await settle();
    loadMore(); await settle();
    assert.deepEqual(more, [["portal", 100]]);
    await choose("store-sort", "Alfabetico decrescente (Z-A)");
    assert.deepEqual(calls.at(-1), ["portal", { sort: "name_desc", availability: "all" }]);
    loadMore(); await settle();
    assert.equal(more.length, 1, "changing sort must restart at 20 visible games");
    loadMore(); await settle();
    await choose("store-availability", "Solo verified");
    assert.deepEqual(calls.at(-1), ["portal", { sort: "name_desc", availability: "verified" }]);
    loadMore(); await settle();
    assert.equal(more.length, 1, "changing availability must restart at 20 visible games");
    await choose("store-availability", "Solo disponibili");
    assert.deepEqual(calls.at(-1), ["portal", { sort: "name_desc", availability: "available" }]);
    globalThis.storeFilterFixture.source.set({ data: { manifest: { verified: [], unverified: [] }, warning: null } });
    await searchDelay();
    assert.deepEqual(calls.at(-1), ["portal", { sort: "name_desc", availability: "available" }]);
    globalThis.storeFilterFixture.catalog.set({ results, total: 120, status: "ready", refreshing: false, error: "Offline", nextOffset: null });
    await settle();
    assert.equal(notifications.at(-1)?.[0], "Offline", "a failed refresh must offer retry while retaining cached games");
    notifications.at(-1)[2].onRetry();
    assert.deepEqual(calls.at(-1), ["portal", { sort: "name_desc", availability: "available" }, true]);
  } finally { await unmount(instance); target.remove(); }
});
