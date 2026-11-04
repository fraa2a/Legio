import assert from "node:assert/strict";
import { test } from "node:test";
import { JSDOM } from "jsdom";
import { compile } from "svelte/compiler";
import { createModuleLoader, dataModule } from "./test_module_loader.mjs";

const dom = new JSDOM("<!doctype html><html><body></body></html>", { pretendToBeVisual: true });
for (const key of [
  "window", "document", "navigator", "Node", "Text", "Comment", "Element", "HTMLElement", "HTMLInputElement",
  "HTMLButtonElement", "HTMLMediaElement", "Event", "KeyboardEvent", "MouseEvent", "MutationObserver",
  "CustomEvent", "getComputedStyle", "requestAnimationFrame", "cancelAnimationFrame",
]) {
  Object.defineProperty(globalThis, key, { configurable: true, value: dom.window[key] });
}
globalThis.matchMedia = () => ({ matches: false });
HTMLElement.prototype.showPopover = function () {};
Element.prototype.scrollIntoView = function () {};
HTMLElement.prototype.hidePopover = function () {};
// Transitions and the select list finish right after they start.
HTMLElement.prototype.animate = function () {
  const animation = { cancel() {}, finished: Promise.resolve() };
  queueMicrotask(() => animation.onfinish?.());
  return animation;
};
const { mount, unmount, flushSync } = await import("svelte");
const settle = async () => { for (let i = 0; i < 6; i++) { await new Promise((resolve) => setImmediate(resolve)); flushSync(); } };

const storeImport = `import { writable } from "${import.meta.resolve("svelte/store")}";`;
const svelteModule = async (loader, source) => dataModule(await loader.resolveImports(compile(source, { generate: "client" }).js.code, "stub.svelte"));
const loader = createModuleLoader({
  "src/lib/i18n": dataModule(storeImport + 'export const language=writable("it"); export const t=(m,l,v=[])=>m.replace(/\\{(\\d+)\\}/g,(_,i)=>String(v[i]??""));'),
  "src/lib/services/invoke": dataModule("export const invoke=(...args)=>globalThis.browserInvoke(...args);"),
  "@tauri-apps/api/event": dataModule("export const listen=async()=>()=>{};"),
  "src/lib/features/modding/ModIcon.svelte": await svelteModule(createModuleLoader({}), "<span></span>"),
});
const { load } = loader;
const model = await import(await load("src/lib/features/modding/modding-model.ts"));
const panel = await import(await load("src/lib/features/modding/mod-panel-model.ts"));

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

const summary = (fullName, extra = {}) => {
  const [owner, name] = fullName.split("-");
  return {
    fullName, owner, name, packageUrl: `https://thunderstore.io/c/lc/p/${owner}/${name}/`, categories: ["Suits"], isDeprecated: false,
    isPinned: false, hasNsfwContent: false, ratingScore: 3, dateCreated: "2024-01-01", dateUpdated: "2024-01-02",
    totalDownloads: 10, latestVersion: "2.0.0", description: "Suits for everyone", icon: "", ...extra,
  };
};

test("mod tabs open, refocus, close and keep at most eight", () => {
  let state = panel.openModTab(panel.emptyModTabs, "A-One", summary("A-One"));
  state = panel.openModTab(state, "B-Two");
  assert.deepEqual([state.tabs.map((tab) => tab.key), state.active], [["a-one", "b-two"], "b-two"]);
  state = panel.openModTab(state, "a-ONE");
  assert.deepEqual([state.tabs.length, state.active], [2, "a-one"], "reopening shows the existing tab");
  assert.equal(panel.modTabTitle(state.tabs[1]), "Two");
  state = panel.setModTabSummary(state, summary("B-Two", { name: "Two_Better" }));
  assert.equal(panel.modTabTitle(state.tabs[1]), "Two Better");
  assert.equal(panel.adjacentModTab(state, "b-two", 1), "a-one", "arrows wrap around");

  for (const name of ["C", "D", "E", "F", "G", "H"]) state = panel.openModTab(state, `${name}-Mod`);
  assert.equal(state.tabs.length, panel.modTabLimit);
  state = panel.focusModTab(state, "a-one");
  state = panel.openModTab(state, "I-Mod");
  assert.equal(state.tabs.length, panel.modTabLimit);
  assert.ok(!state.tabs.some((tab) => tab.key === "b-two"), "the least recently shown tab goes");
  assert.ok(state.tabs.some((tab) => tab.key === "a-one"));

  state = panel.focusModTab(state, "e-mod");
  state = panel.closeModTab(state, "e-mod");
  assert.equal(state.active, "f-mod", "the right neighbour is shown next");
  state = panel.closeModTab(state, "i-mod");
  assert.equal(state.active, "f-mod", "closing a hidden tab keeps the shown one");
  for (const tab of state.tabs) state = panel.closeModTab(state, tab.key);
  assert.deepEqual(state, panel.emptyModTabs);
});

test("collapsing the panel keeps its tabs and opening a mod expands it again", () => {
  assert.equal(panel.modPanelState(panel.emptyModTabs), "none");
  assert.equal(panel.setModPanelCollapsed(panel.emptyModTabs, true), panel.emptyModTabs, "nothing to collapse without tabs");
  let state = panel.openModTab(panel.openModTab(panel.emptyModTabs, "A-One"), "B-Two");
  assert.equal(panel.modPanelState(state), "open");
  state = panel.setModPanelCollapsed(state, true);
  assert.deepEqual([panel.modPanelState(state), state.tabs.length, state.active], ["collapsed", 2, "b-two"]);
  assert.equal(panel.setModPanelCollapsed(state, true), state);
  assert.equal(panel.modPanelState(panel.setModPanelCollapsed(state, false)), "open");
  assert.equal(panel.modPanelState(panel.openModTab(state, "C-Three")), "open", "a new tab shows the panel");
  assert.equal(panel.modPanelState(panel.openModTab(state, "a-one")), "open", "an open mod is shown again");
  assert.equal(panel.modPanelState(panel.focusModTab(state, "a-one")), "open");
  assert.deepEqual(panel.closeModTab(panel.closeModTab(state, "a-one"), "b-two"), panel.emptyModTabs);
});

test("tags beyond the limit are counted", () => {
  assert.deepEqual(panel.splitModTags(["a", "b"]), { shown: ["a", "b"], hidden: [] });
  assert.deepEqual(panel.splitModTags(["a", "b", "c", "d", "e"]), { shown: ["a", "b", "c"], hidden: ["d", "e"] });
});

test("the install action names the version it installs", () => {
  assert.equal(panel.installActionLabel(null, "2.0.0", true, "it"), "Installa");
  assert.equal(panel.installActionLabel(null, "1.0.0", false, "it"), "Installa la versione 1.0.0");
  assert.equal(panel.installActionLabel("1.0.0", "2.0.0", true, "it"), "Aggiorna a 2.0.0");
  assert.equal(panel.installActionLabel("2.0.0", "1.0.0", false, "it"), "Installa la versione 1.0.0");
  assert.equal(panel.installActionLabel("2.0.0", "2.0.0", true, "it"), "Installata");
});

test("search results open in panel tabs with readme, changelog and version install", async () => {
  const calls = [];
  const version = (fullName, versionNumber, dependencies = []) => ({
    fullName, versionNumber, description: `Version ${versionNumber}`, dependencies, fileSize: 2048, websiteUrl: null, dateCreated: "2024-01-01", downloads: 5,
  });
  const details = (fullName) => ({
    ...summary(fullName),
    donationLink: null,
    versions: fullName === "Owner-More_Suits"
      ? [version(fullName, "2.0.0", ["Owner-Core-1.0.0"]), version(fullName, "1.0.0")]
      : [version(fullName, "1.0.0")],
  });
  globalThis.browserInvoke = async (command, args) => {
    calls.push([command, args]);
    if (command === "search_mods") return { total: 2, items: [summary("Owner-More_Suits"), summary("Other-Hats")] };
    if (command === "list_mod_categories") return [];
    if (command === "list_profile_mods" || command === "list_mod_updates") return [];
    if (command === "get_mod_package") return details(args.fullName);
    if (command === "get_mod_package_document") return `<p>${args.document} of ${args.fullName} ${args.version}</p>`;
    if (command === "install_mods") return { installed: ["Owner-More_Suits-1.0.0"], skipped: [], warnings: [] };
    throw new Error(`unexpected ${command}`);
  };
  const browser = `<script>
    import ModBrowser from "./src/lib/features/modding/ModBrowser.svelte";
    import { emptyModTabs } from "./src/lib/features/modding/mod-panel-model";
    let tabs = $state(emptyModTabs);
  </script>
  <ModBrowser gameId="g" ecosystemGameId="lethal-company" profile={{ id: "p", gameId: "g", name: "Main", createdAt: 0 }} disabled={false} {tabs} onTabsChange={(next) => (tabs = next)} />`;
  const Component = (await import(await svelteModule(loader, browser))).default;
  const target = document.createElement("div");
  document.body.append(target);
  const instance = mount(Component, { target });
  const button = (label, root = target) => [...root.querySelectorAll("button")].find((item) => item.textContent.trim() === label || item.getAttribute("aria-label") === label);
  const tabs = () => [...target.querySelectorAll('[role="tab"]')].map((tab) => [tab.textContent.trim(), tab.getAttribute("aria-selected")]);
  const shown = () => target.querySelector('[role="tabpanel"]:not([hidden])');
  try {
    await new Promise((resolve) => setTimeout(resolve, 350)); await settle();
    assert.equal(target.querySelector("aside"), null, "the panel stays closed until a mod is opened");

    button("Dettagli di More Suits").click(); await settle();
    assert.deepEqual(tabs(), [["More Suits", "true"]]);
    assert.ok(target.querySelector('[role="tablist"][aria-label="Mod aperte"]'));
    assert.ok(shown().textContent.includes("readme of Owner-More_Suits 2.0.0"), "the readme of the latest version comes first");

    assert.deepEqual([...shown().querySelector("h2").parentElement.parentElement.querySelectorAll("ul li")].map((item) => item.getAttribute("title").split(":")[0]), ["Ultimo rilascio", "Download totali", "Valutazione", "Dimensione"], "the stats sit in one compact row");
    assert.ok(shown().querySelector("h2").parentElement.textContent.includes("Suits"), "the tags sit next to the name");

    shown().querySelector('input[value="changelog"]').click(); await settle();
    assert.ok(shown().textContent.includes("changelog of Owner-More_Suits 2.0.0"));

    button("Dettagli di Hats").click(); await settle();
    assert.deepEqual(tabs(), [["More Suits", "false"], ["Hats", "true"]]);
    button("Dettagli di More Suits").click(); await settle();
    assert.deepEqual(tabs(), [["More Suits", "true"], ["Hats", "false"]], "reopening focuses the existing tab");
    assert.ok(shown().textContent.includes("changelog of Owner-More_Suits"), "each tab keeps its own view");

    target.querySelector('[aria-controls="mod-version-owner-more_suits-options"]').click(); await settle();
    [...target.querySelectorAll('[role="option"]')].find((option) => option.textContent.trim() === "1.0.0").click(); await settle();
    button("Installa la versione 1.0.0", shown()).click(); await settle();
    assert.deepEqual(calls.find(([command]) => command === "install_mods"), ["install_mods", { profileId: "p", packages: [{ fullName: "Owner-More_Suits", version: "1.0.0" }] }]);

    // The old version has no dependencies, so go back to the latest one.
    target.querySelector('[aria-controls="mod-version-owner-more_suits-options"]').click(); await settle();
    [...target.querySelectorAll('[role="option"]')].find((option) => option.textContent.trim() === "2.0.0 (ultima)").click(); await settle();
    shown().querySelector('input[value="dependencies"]').click(); await settle();
    button("Apri Core 1.0.0", shown()).click(); await settle();
    assert.deepEqual(tabs().at(-1), ["Core", "true"], "dependencies open in a new tab");

    target.querySelector('[role="tab"][aria-selected="true"]').dispatchEvent(new MouseEvent("auxclick", { button: 1, bubbles: true }));
    await settle();
    assert.deepEqual(tabs(), [["More Suits", "false"], ["Hats", "true"]], "middle click closes the tab");

    button("Comprimi le mod aperte").click(); await settle();
    assert.deepEqual(tabs(), [["More Suits", "false"], ["Hats", "true"]], "collapsing keeps the open tabs");
    assert.ok(target.querySelector("aside").inert, "the collapsed panel is out of reach");
    button("Mostra le mod aperte").click(); await settle();
    assert.equal(target.querySelector("aside").inert, false);
    assert.deepEqual(tabs(), [["More Suits", "false"], ["Hats", "true"]], "expanding keeps the active tab");
    assert.equal(button("Mostra le mod aperte"), undefined);

    button("Chiudi Hats").click(); await settle();
    button("Chiudi More Suits").click(); await settle();
    assert.equal(target.querySelector("aside"), null, "the panel slides away with its last tab");
  } finally {
    await unmount(instance);
    target.remove();
    delete globalThis.browserInvoke;
  }
});

test("installed mods open in their own tab set, including local mods", async () => {
  const mod = (fullName, extra = {}) => ({
    profileId: "p", fullName, version: "1.0.0", enabled: true, onlineSource: true, installedAt: 0, installOrder: 0, isLoader: false, dependencies: [], ...extra,
  });
  const mods = [
    mod("BepInEx-BepInExPack", { isLoader: true, version: "5.4.0" }),
    mod("Owner-More_Suits", { version: "1.0.0", installOrder: 1 }),
    mod("Me-Homebrew", { onlineSource: false, version: "0.3.0", installOrder: 2, dependencies: ["Owner-More_Suits"] }),
  ];
  const calls = [];
  const version = (fullName, versionNumber) => ({
    fullName, versionNumber, description: `Version ${versionNumber}`, dependencies: [], fileSize: 2048, websiteUrl: null, dateCreated: "2024-01-01", downloads: 5,
  });
  globalThis.browserInvoke = async (command, args) => {
    calls.push([command, args]);
    if (command === "list_profile_mods") return mods;
    if (command === "list_mod_updates" || command === "list_mod_categories") return [];
    if (command === "search_mods") return { total: 1, items: [summary("Owner-More_Suits")] };
    if (command === "get_mod_package") {
      if (args.fullName !== "Owner-More_Suits") return null;
      return { ...summary(args.fullName), donationLink: null, versions: [version(args.fullName, "2.0.0"), version(args.fullName, "1.0.0")] };
    }
    if (command === "get_mod_package_document") return `<p>${args.document} ${args.version}</p>`;
    if (command === "install_mods") return { installed: [], skipped: [], warnings: [] };
    throw new Error(`unexpected ${command}`);
  };
  const host = `<script>
    import InstalledModsList from "./src/lib/features/modding/InstalledModsList.svelte";
    import ModBrowser from "./src/lib/features/modding/ModBrowser.svelte";
    import { emptyModTabs } from "./src/lib/features/modding/mod-panel-model";
    let browse = $state(emptyModTabs);
    let installedTabs = $state(emptyModTabs);
    let showInstalled = $state(true);
    const profile = { id: "p", gameId: "g", name: "Main", createdAt: 0 };
  </script>
  <button onclick={() => (showInstalled = !showInstalled)}>switch</button>
  {#if showInstalled}
    <InstalledModsList gameId="g" ecosystemGameId="lethal-company" {profile} locked={false} busy={false} tabs={installedTabs} onTabsChange={(next) => (installedTabs = next)} onBrowse={() => {}} />
  {:else}
    <ModBrowser gameId="g" ecosystemGameId="lethal-company" {profile} disabled={false} tabs={browse} onTabsChange={(next) => (browse = next)} />
  {/if}`;
  const Component = (await import(await svelteModule(loader, host))).default;
  const target = document.createElement("div");
  document.body.append(target);
  const instance = mount(Component, { target });
  const button = (label, root = target) => [...root.querySelectorAll("button")].find((item) => item.textContent.trim() === label || item.getAttribute("aria-label") === label);
  const tabs = () => [...target.querySelectorAll('[role="tab"]')].map((tab) => [tab.textContent.trim(), tab.getAttribute("aria-selected")]);
  const shown = () => target.querySelector('[role="tabpanel"]:not([hidden])');
  try {
    await settle();
    assert.equal(target.querySelector("aside"), null);

    button("Dettagli di More Suits").click(); await settle();
    assert.deepEqual(tabs(), [["More Suits", "true"]]);
    assert.ok(button("Dettagli di More Suits").closest("li").className.includes("ring-legio-accent"), "the open mod's row is ringed");
    assert.ok(shown().textContent.includes("readme 1.0.0"), "the installed version is shown first");
    assert.ok(button("Installata", shown()) !== undefined, "the installed version has no install action");

    button("Dettagli di Homebrew").click(); await settle();
    assert.deepEqual(tabs(), [["More Suits", "false"], ["Homebrew", "true"]]);
    const local = shown().textContent;
    assert.ok(local.includes("0.3.0") && local.includes("non è su Thunderstore") && local.includes("Locale"));
    assert.ok(button("Apri More Suits", shown()), "local dependencies open from the installed data");
    assert.ok(!calls.some(([command, args]) => command === "get_mod_package" && args.fullName === "Me-Homebrew"), "no catalog lookup for a local mod");

    button("Dettagli di BepInExPack").click(); await settle();
    assert.ok(shown().textContent.includes("5.4.0") && shown().textContent.includes("Loader"), "a loader missing from the catalog still opens");

    button("switch").click(); await new Promise((resolve) => setTimeout(resolve, 350)); await settle();
    assert.equal(target.querySelector("aside"), null, "the browser keeps its own, empty tab set");
    button("Dettagli di More Suits").click(); await settle();
    assert.deepEqual(tabs(), [["More Suits", "true"]]);
    assert.ok(shown().textContent.includes("Aggiorna a 2.0.0"), "the browser opens on the latest version");

    button("switch").click(); await settle();
    assert.equal(tabs().length, 3, "the installed tab set survives switching sections");
  } finally {
    await unmount(instance);
    target.remove();
    delete globalThis.browserInvoke;
  }
});

test("search results are paged twenty at a time and the filters restart at page one", async () => {
  const offsets = [];
  globalThis.browserInvoke = async (command, args) => {
    if (command === "search_mods") {
      const { offset, limit } = args.query;
      offsets.push([offset, limit]);
      return { total: 45, items: Array.from({ length: Math.min(limit, 45 - offset) }, (_, index) => summary(`Owner-Mod${offset + index}`)) };
    }
    if (["list_mod_categories", "list_profile_mods", "list_mod_updates"].includes(command)) return [];
    throw new Error(`unexpected ${command}`);
  };
  const host = `<script>
    import ModBrowser from "./src/lib/features/modding/ModBrowser.svelte";
    import { emptyModTabs } from "./src/lib/features/modding/mod-panel-model";
    let tabs = $state(emptyModTabs);
  </script>
  <ModBrowser gameId="g" ecosystemGameId="lethal-company" profile={{ id: "p", gameId: "g", name: "Main", createdAt: 0 }} disabled={false} {tabs} onTabsChange={(next) => (tabs = next)} />`;
  const Component = (await import(await svelteModule(loader, host))).default;
  const target = document.createElement("div");
  document.body.append(target);
  const instance = mount(Component, { target });
  const button = (label) => target.querySelector(`button[aria-label="${label}"]`);
  const rows = () => target.querySelectorAll('button[aria-label^="Dettagli di "]').length;
  const current = () => target.querySelector('nav button[aria-pressed="true"]')?.getAttribute("aria-label");
  try {
    await new Promise((resolve) => setTimeout(resolve, 350)); await settle();
    assert.deepEqual(offsets, [[0, 20]]);
    assert.equal(rows(), 20);
    assert.deepEqual([current(), button("Pagina 3") !== null, button("Pagina precedente").disabled], ["Pagina 1", true, true]);

    button("Pagina 3").click(); await settle();
    assert.deepEqual(offsets.at(-1), [40, 20]);
    assert.equal(rows(), 5, "the last page holds the remainder, not the earlier pages");
    assert.deepEqual([current(), button("Pagina successiva").disabled], ["Pagina 3", true]);

    button("Pagina precedente").click(); await settle();
    assert.deepEqual(offsets.at(-1), [20, 20]);
    assert.equal(current(), "Pagina 2");
  } finally { await unmount(instance); target.remove(); }
});
