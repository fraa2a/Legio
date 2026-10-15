import assert from "node:assert/strict";
import { test } from "node:test";
import { JSDOM } from "jsdom";
import { compile } from "svelte/compiler";
import { createModuleLoader, dataModule } from "./test_module_loader.mjs";

const dom = new JSDOM("<!doctype html><html><body></body></html>", { pretendToBeVisual: true });
for (const key of ["window", "document", "navigator", "HTMLElement", "HTMLInputElement", "Element", "Node", "Text", "Comment", "Event", "MutationObserver"]) {
  Object.defineProperty(globalThis, key, { configurable: true, value: dom.window[key] });
}
const { mount, unmount, flushSync } = await import("svelte");
const { writable } = await import("svelte/store");
const settle = async () => { for (let i = 0; i < 5; i++) { await new Promise(resolve => setImmediate(resolve)); flushSync(); } };

test("transfer asks only for a destination library and stays disabled while the game runs", async () => {
  const launch = writable(new Map());
  const game = { id: "game", name: "My Game", steamInstallPath: null, executablePath: "/old/Game/Game.exe", installationRoot: "/old/Game" };
  const calls = [];
  globalThis.libraryFixture = { launch, calls, game };
  const storeImport = `import { writable } from "${import.meta.resolve("svelte/store")}";`;
  const stub = async source => dataModule(await createModuleLoader({}).resolveImports(compile(source, { generate: "client" }).js.code, "stub.svelte"));
  const empty = await stub("<div></div>");
  const mocks = {
    "src/lib/i18n": dataModule(storeImport + 'export const language=writable("it"); export const t=value=>value;'),
    "src/lib/stores/app-info": dataModule(storeImport + 'export const appInfo=writable({data:{platform:"windows"}});'),
    "src/lib/stores/launch": dataModule('export const launchStateByGame=globalThis.libraryFixture.launch;'),
    "src/lib/stores/playtime": dataModule(storeImport + 'export const playtime=writable({data:[{gameId:"game",totalMilliseconds:3600000,activeSessions:0}]}); playtime.load=async()=>{};'),
    "src/lib/services/playtime": dataModule('export const setGamePlaytime=async(...args)=>{globalThis.libraryFixture.calls.push(args);};'),
    "src/lib/stores/games": dataModule('export const reconcileGame=()=>{};'),
    "src/lib/services/dialog": dataModule('export const pickGameDirectory=async()=>"/new-library";'),
    "src/lib/services/local-state": dataModule('export const createDesktopShortcut=async()=>({}); export const transferGame=async(...args)=>{globalThis.libraryFixture.calls.push(args);return {game:globalThis.libraryFixture.game,warning:null};};'),
    "src/lib/components/ui/Dialog.svelte": await stub('<script>let {children}=$props();</script><div>{@render children?.()}</div>'),
  };
  for (const panel of ["GameArtworkPanel", "GameDetailsPanel", "ExecutablePanel", "GameSettingsPanel", "SteamAccountPanel", "SteamLaunchSettings"]) {
    mocks[`src/lib/features/library/${panel}.svelte`] = empty;
  }
  const loader = createModuleLoader(mocks);
  const Component = (await import(await loader.load("src/lib/features/library/GameSettingsDialog.svelte"))).default;
  const target = document.createElement("div"); document.body.append(target);
  const instance = mount(Component, { target, props: { game, onClose() {}, onRemoved() {} } });
  try {
    await settle();
    [...target.querySelectorAll("button")].find(button => button.textContent.trim() === "Posizioni").click(); await settle();
    assert.equal(target.querySelector("#transfer-source"), null);
    [...target.querySelectorAll("button")].find(button => button.textContent.includes("Sfoglia destinazione")).click(); await settle();
    assert.equal(target.querySelector("#transfer-destination").value, "/new-library");
    const button = [...target.querySelectorAll("button")].find(button => button.textContent.trim() === "Trasferisci gioco");
    assert.equal(button.disabled, false);
    launch.set(new Map([[game.id, { status: "running" }]])); await settle();
    assert.equal(button.disabled, true);
    assert.equal(calls.length, 0);
    launch.set(new Map()); await settle();
    button.click(); await settle();
    assert.deepEqual(calls, [[game.id, "/new-library"]]);
    [...target.querySelectorAll("button")].find(button => button.textContent.trim() === "Zona pericolosa").click(); await settle();
    const hours = target.querySelector("#playtime-hours");
    assert.ok(hours, "danger settings expose an editable playtime total");
    assert.equal(hours.value, "1");
    hours.value = "2";
    hours.dispatchEvent(new dom.window.Event("input", {bubbles:true})); await settle();
    [...target.querySelectorAll("button")].find(button => button.textContent.trim() === "Imposta tempo di gioco").click(); await settle();
    [...target.querySelectorAll("button")].find(button => button.textContent.trim() === "Conferma modifica").click(); await settle();
    assert.deepEqual(calls.at(-1), [game.id, 7200000]);
  } finally { await unmount(instance); target.remove(); delete globalThis.libraryFixture; }
});
