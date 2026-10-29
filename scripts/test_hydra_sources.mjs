import assert from "node:assert/strict";
import { test } from "node:test";
import { JSDOM } from "jsdom";
import { createModuleLoader, dataModule } from "./test_module_loader.mjs";

const dom = new JSDOM("<!doctype html><html><body></body></html>", { pretendToBeVisual: true });
for (const key of ["window", "document", "navigator", "Node", "Text", "Comment", "Element", "HTMLElement", "HTMLMediaElement", "HTMLInputElement", "HTMLFormElement", "Event", "KeyboardEvent", "MouseEvent", "CustomEvent", "getComputedStyle"]) {
  Object.defineProperty(globalThis, key, { configurable: true, value: dom.window[key] });
}
globalThis.matchMedia = () => ({ matches: false });
const { mount, unmount, flushSync } = await import("svelte");
const settle = async () => { flushSync(); await new Promise(setImmediate); flushSync(); };

test("source-distinct versions retain names and stable selection IDs", async () => {
  const { load } = createModuleLoader({});
  const model = await import(await load("src/lib/features/store/source-status.ts"));
  const entries = ["one", "two", "three"].map((sourceId, index) => ({
    id: "release", sourceId, steamAppId: 400, name: "Portal", release: { version: String(index) },
    download: { url: "https://example.test/file.zip", sizeBytes: null },
  }));
  const manifest = { verified: [], unverified: entries };
  assert.equal(model.sourceReleasesFor(manifest, 400).length, 3);
  assert.equal(new Set(entries.map(model.sourceReleaseId)).size, 3);
  const sources = [{ id: "one", name: "SteamGG" }, { id: "two", name: "Other" }, { id: "three", name: "SteamGG" }];
  assert.deepEqual(entries.map(entry => model.sourceNameFor(entry, sources)), ["SteamGG", "Other", "SteamGG"]);
});

test("release picker shows source badges alongside names and keeps the Steam badge", async () => {
  const { load } = createModuleLoader({
    "src/lib/utils/motion": dataModule("export const reducedMotion = true;"),
    "src/lib/i18n": dataModule(`import { writable } from ${JSON.stringify(import.meta.resolve("svelte/store"))}; export const language = writable("it"); export const t = value => value;`),
    "src/lib/features/library/SteamArtwork.svelte": dataModule("export default function () {}"),
    "src/lib/components/ui/Icon.svelte": dataModule("export default function () {}"),
  });
  const Component = (await import(await load("src/lib/features/library/ReleasePicker.svelte"))).default;
  const target = document.createElement("div"); document.body.append(target);
  let selected;
  HTMLElement.prototype.showPopover = function () {};
  const instance = mount(Component, { target, props: { steamAppId: null, selectedId: "a", options: [
    { id: "a", name: "Portal", subtitle: "1", sourceName: "SteamGG" },
    { id: "b", name: "Portal", subtitle: "2", sourceName: "Other" },
    { id: "c", name: "Portal", subtitle: "3", isSteam: true },
  ], onSelect: id => { selected = id; } } });
  try {
    await settle();
    assert.match(target.querySelector("button").textContent, /SteamGG/);
    target.querySelector("button").click(); await settle();
    assert.match(target.textContent, /Other/);
    assert.match(target.textContent, /STEAM/);
    [...target.querySelectorAll('[role="option"]')].find(option => option.textContent.includes("Other")).click();
    assert.equal(selected, "b");
  } finally { await unmount(instance); target.remove(); }
});


test("local source controls distinguish file catalogs and preserve cancellation", async () => {
  const calls = [];
  globalThis.localSourceControls = { chosen: null, calls };
  const storeModule = dataModule(`import { writable } from ${JSON.stringify(import.meta.resolve("svelte/store"))};
    export const source = writable({data:{sources:[{id:"local",url:"file:///tmp/steamrip.json",name:"SteamRIP",format:"hydra",isLocal:true,gameCount:2,stale:false}],warning:null},error:null});
    export const sourceBusy=writable(false),sourceActionError=writable(null),sourceRefreshError=writable(null);
    export const addSource=async()=>true,removeSource=async()=>true,refreshSource=async()=>{};
    export const addLocalSource=async(path)=>{globalThis.localSourceControls.calls.push(path);return true;};`);
  const { load } = createModuleLoader({
    "src/lib/stores/source": storeModule,
    "src/lib/services/dialog": dataModule("export const pickSourceFile=async()=>globalThis.localSourceControls.chosen;"),
    "src/lib/services/legio-source": dataModule("export const observeSourceImportProgress=async()=>()=>{};"),
    "src/lib/stores/source-links": dataModule(`import { writable } from ${JSON.stringify(import.meta.resolve("svelte/store"))}; export const sourceLinkRegistrationError=writable(null);`),
    "src/lib/i18n": dataModule(`import { writable } from ${JSON.stringify(import.meta.resolve("svelte/store"))}; export const language=writable("it");export const t=value=>value;`),
  });
  const Component=(await import(await load("src/lib/features/settings/SourceSettings.svelte"))).default;
  const target=document.createElement("div");document.body.append(target);
  const instance=mount(Component,{target});
  try {
    await settle();
    assert.match(target.textContent,/file locale/);
    const button=[...target.querySelectorAll("button")].find(button=>button.textContent.includes("Scegli file JSON"));
    button.click();await settle();assert.deepEqual(calls,[]);
    globalThis.localSourceControls.chosen="/tmp/steamrip.json";
    button.click();await settle();assert.deepEqual(calls,["/tmp/steamrip.json"]);
  } finally { await unmount(instance);target.remove(); }
});
