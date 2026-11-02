import assert from "node:assert/strict";
import { test } from "node:test";
import { JSDOM } from "jsdom";
import { get, writable } from "svelte/store";
import { createModuleLoader, dataModule } from "./test_module_loader.mjs";

const dom = new JSDOM("<!doctype html><html><body></body></html>", { pretendToBeVisual: true });
for (const key of ["window", "document", "navigator", "Node", "Text", "Comment", "Element", "HTMLElement", "HTMLMediaElement", "Event", "KeyboardEvent", "MouseEvent"]) {
  Object.defineProperty(globalThis, key, { configurable: true, value: dom.window[key] });
}
HTMLElement.prototype.showPopover = function () {};
Element.prototype.scrollIntoView = function () {};
globalThis.matchMedia = () => ({ matches: true });
const { mount, unmount, flushSync } = await import("svelte");
const settle = async () => { flushSync(); await new Promise(resolve => setImmediate(resolve)); flushSync(); };
const searchDelay = async () => { await settle(); await new Promise(resolve => setTimeout(resolve, 330)); await settle(); };

test("pagination model slices twenty entries and elides distant pages", async () => {
  const { pageCountFor, pageItems, pageNumbers } = await import(await createModuleLoader({}).load("src/lib/utils/pagination.ts"));
  const items = Array.from({ length: 45 }, (_, index) => index);
  assert.deepEqual([pageCountFor(0), pageCountFor(20), pageCountFor(21)], [1, 1, 2]);
  assert.deepEqual([pageItems(items, 1).length, pageItems(items, 3), pageItems(items, 4)], [20, [40, 41, 42, 43, 44], []]);
  assert.deepEqual(pageNumbers(1, 3), [1, 2, 3]);
  assert.deepEqual(pageNumbers(1, 10), [1, 2, "gap", 10]);
  assert.deepEqual(pageNumbers(5, 10), [1, "gap", 4, 5, 6, "gap", 10]);
  assert.deepEqual(pageNumbers(3, 10), [1, 2, 3, 4, "gap", 10]);
  assert.deepEqual(pageNumbers(10, 10), [1, "gap", 9, 10]);
});

test("store controls combine filters and reset pagination when either choice changes", async () => {
  const calls = [];
  const more = [];
  const notifications = [];
  globalThis.storeFilterNotifications = notifications;
  const results = Array.from({ length: 80 }, (_, index) => ({ steamAppId: index + 1, name: `Game ${index + 1}` }));
  globalThis.storeFilterFixture = {
    catalog: writable({ results, total: 120, status: "ready", refreshing: false, error: null, nextOffset: null }),
    settings: writable({ data: { storeSort: "relevance", storeAvailability: "all" }, status: "ready" }),
    source: writable({ data: { manifest: null, warning: null } }),
    query: writable("portal"),
    search: (...args) => calls.push(args), more: (...args) => more.push(args),
  };
  const stores = import.meta.resolve("svelte/store");
  const { load } = createModuleLoader({
    "src/lib/i18n": dataModule(`import { writable } from ${JSON.stringify(stores)}; export const language = writable("it"); export const t = (value, language, values = []) => value.replace(/\\{(\\d+)\\}/g, (_, index) => String(values[index] ?? ""));`),
    "src/lib/stores/catalog": dataModule("export const catalog = globalThis.storeFilterFixture.catalog; export const runCatalogSearch = (...args) => globalThis.storeFilterFixture.search(...args); export const loadMoreCatalog = (...args) => globalThis.storeFilterFixture.more(...args);"),
    "src/lib/stores/library-ui": dataModule("export const storeQuery = globalThis.storeFilterFixture.query;"),
    "src/lib/stores/source": dataModule(`import { writable } from ${JSON.stringify(stores)}; export const source = globalThis.storeFilterFixture.source; export const sourceRefreshError = writable(null); export const refreshSource = async () => {};`),
    "src/lib/stores/settings": dataModule(`import { get } from ${JSON.stringify(stores)}; export const settings = globalThis.storeFilterFixture.settings; export const updateSettings = async patch => { const current = get(settings).data; const updated = { ...current, ...patch }; settings.set({ data: updated, status: "ready" }); return updated; };`),
    "src/lib/stores/navigation": dataModule(`import { writable } from ${JSON.stringify(stores)}; export const selectedStoreGame = writable(null); export const openStoreGame = () => {};`),
    "src/lib/stores/toast": dataModule("export const showToast = (...args) => globalThis.storeFilterNotifications.push(args);"),
    "src/lib/features/store/StoreGameCard.svelte": dataModule("export default function () {}"),
    "src/lib/features/library/GameDetailView.svelte": dataModule("export default function () {}"),
  });
  const StoreView = (await import(await load("src/lib/features/store/StoreView.svelte"))).default;
  const target = document.createElement("div");
  document.body.append(target);
  const instance = mount(StoreView, { target });
  const pageButton = label => target.querySelector(`button[aria-label="${label}"]`);
  const loadMore = () => pageButton("Pagina successiva").click();
  const current = () => target.querySelector('nav button[aria-pressed="true"]')?.getAttribute("aria-label");
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
    assert.equal(current(), "Pagina 1");
    loadMore(); await settle();
    loadMore(); await settle();
    assert.equal(current(), "Pagina 3");
    assert.deepEqual(more, []);
    loadMore(); await settle();
    assert.equal(current(), "Pagina 4");
    assert.deepEqual(more, [["portal", 100]]);
    pageButton("Pagina precedente").click(); await settle();
    assert.equal(current(), "Pagina 3");
    assert.equal(more.length, 1, "going back never loads more games");
    await choose("store-sort", "Alfabetico decrescente (Z-A)");
    assert.deepEqual(calls.at(-1), ["portal", { sort: "name_desc", availability: "all" }]);
    assert.equal(current(), "Pagina 1", "changing sort must restart at the first page");
    loadMore(); await settle();
    assert.equal(more.length, 1);
    loadMore(); await settle();
    await choose("store-availability", "Solo verified");
    assert.deepEqual(calls.at(-1), ["portal", { sort: "name_desc", availability: "verified" }]);
    loadMore(); await settle();
    assert.equal(current(), "Pagina 2");
    assert.equal(more.length, 1, "changing availability must restart at the first page");
    await choose("store-availability", "Solo disponibili");
    assert.deepEqual(calls.at(-1), ["portal", { sort: "name_desc", availability: "available" }]);
    assert.deepEqual(get(globalThis.storeFilterFixture.settings).data, { storeSort: "name_desc", storeAvailability: "available" });
    globalThis.storeFilterFixture.source.set({ data: { manifest: { verified: [], unverified: [] }, warning: null } });
    await searchDelay();
    assert.deepEqual(calls.at(-1), ["portal", { sort: "name_desc", availability: "available" }]);
    globalThis.storeFilterFixture.catalog.set({ results, total: 120, status: "ready", refreshing: false, error: "Offline", nextOffset: null });
    await settle();
    assert.match(target.querySelector('[role="alert"]')?.textContent ?? "", /Offline/, "a failed refresh must offer retry while retaining cached games");
    target.querySelector('[role="alert"] button').click();
    assert.deepEqual(calls.at(-1), ["portal", { sort: "name_desc", availability: "available" }, true]);
    globalThis.storeFilterFixture.catalog.set({ results: results.slice(0, 15), total: 15, status: "ready", refreshing: false, error: null, nextOffset: 50 });
    await settle();
    assert.equal(target.querySelector("nav"), null, "a 15-result search must not offer another page");
  } finally { await unmount(instance); target.remove(); }
});
