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
globalThis.matchMedia = () => ({ matches: false });
const settle = async () => { for (let i = 0; i < 5; i++) { await new Promise(resolve => setImmediate(resolve)); flushSync(); } };

test("uninstall previews the exact directory and requires a second confirmation", async () => {
  const calls = [];
  globalThis.uninstallFixture = { calls };
  const { load } = createModuleLoader({
    "src/lib/i18n": dataModule(`import { writable } from "${import.meta.resolve("svelte/store")}"; export const language=writable("it"); export const t=value=>value;`),
    "src/lib/services/local-state": dataModule('export const getGameUninstallTarget=async id=>{globalThis.uninstallFixture.calls.push(["preview",id]);return "/games/My Game";};'),
    "src/lib/stores/games": dataModule('export const deleteGame=async()=>{}; export const hideSteamGame=async()=>{}; export const saveGame=async()=>{}; export const uninstallGame=async(...args)=>globalThis.uninstallFixture.calls.push(["uninstall",...args]);'),
    "src/lib/stores/settings": dataModule(`import { writable } from "${import.meta.resolve("svelte/store")}"; export const settings=writable({data:{syncSteamGames:true}});`),
    "src/lib/features/library/SteamGameSearch.svelte": dataModule(await createModuleLoader({}).resolveImports(compile('<div></div>', { generate: "client" }).js.code, "stub.svelte")),
    "src/lib/components/ui/Dialog.svelte": dataModule(await createModuleLoader({}).resolveImports(compile('<script>let {open,children,onClose}=$props();</script>{#if open}<section role="dialog">{@render children(onClose)}</section>{/if}', { generate: "client" }).js.code, "stub.svelte")),
  });
  const Component = (await import(await load("src/lib/features/library/GameDetailsPanel.svelte"))).default;
  const target = document.createElement("div"); document.body.append(target);
  const instance = mount(Component, { target, props: { game: { id: "local", name: "My Game", steamAppId: 400, steamInstallPath: null, executablePath: "/games/My Game/game.exe", installationRoot: "/games/My Game", nameOverride: null, automaticName: "My Game" }, section: "danger", onRemoved: () => calls.push(["removed"]) } });
  try {
    await settle();
    assert.deepEqual(calls, []);
    const findButton = label => [...target.querySelectorAll("button")].find(button => button.textContent.trim() === label);
    findButton("Disinstalla gioco").click(); await settle();
    assert.deepEqual(calls, [["preview", "local"]]);
    assert.ok(target.textContent.includes("/games/My Game"));
    assert.ok(target.querySelector('[role="dialog"]'));
    findButton("Annulla").click(); await settle();
    assert.equal(target.querySelector('[role="dialog"]'), null);
    assert.deepEqual(calls, [["preview", "local"]], "cancel never uninstalls files");
    findButton("Disinstalla gioco").click(); await settle();
    const button = findButton("Disinstalla definitivamente");
    assert.ok(button);
    button.click(); await settle();
    assert.deepEqual(calls, [["preview", "local"], ["preview", "local"], ["uninstall", "local", "/games/My Game"], ["removed"]]);
  } finally { await unmount(instance); target.remove(); delete globalThis.uninstallFixture; }
});

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
    "src/lib/stores/playtime": dataModule(storeImport + 'export const playtime=writable({data:[{gameId:"game",totalMilliseconds:3600000,activeSessions:0}]}); playtime.load=async()=>{}; playtime.reload=async()=>{};'),
    "src/lib/services/playtime": dataModule('export const setGamePlaytime=async(...args)=>{globalThis.libraryFixture.calls.push(args);};'),
    "src/lib/stores/games": dataModule('export const reconcileGame=()=>{};'),
    "src/lib/services/dialog": dataModule('export const pickGameDirectory=async()=>"/new-library";'),
    "src/lib/services/local-state": dataModule('export const createDesktopShortcut=async()=>({}); export const transferGame=async(...args)=>{globalThis.libraryFixture.calls.push(args);return {game:globalThis.libraryFixture.game,warning:null};};'),
    "src/lib/components/ui/Dialog.svelte": await stub('<script>let {open,children}=$props();</script>{#if open}<div>{@render children?.()}</div>{/if}'),
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
    const editPlaytime = [...target.querySelectorAll("button")].find(button => button.textContent.trim() === "Imposta tempo di gioco");
    assert.ok(editPlaytime, "the red playtime button stays in danger settings");
    assert.equal(target.querySelector("#playtime-hours"), null, "playtime fields stay in the popup");
    editPlaytime.click(); await settle();
    const hours = target.querySelector("#playtime-hours");
    assert.ok(hours, "the popup exposes editable hours and minutes");
    assert.ok(target.querySelector("#playtime-minutes"));
    assert.equal(hours.value, "1");
    hours.value = "2";
    hours.dispatchEvent(new dom.window.Event("input", {bubbles:true})); await settle();
    [...target.querySelectorAll("button")].find(button => button.textContent.trim() === "Conferma modifica").click(); await settle();
    assert.deepEqual(calls.at(-1), [game.id, 7200000]);
    assert.equal(target.querySelector("#playtime-hours"), null, "the popup closes after saving");
  } finally { await unmount(instance); target.remove(); delete globalThis.libraryFixture; }
});

test("Steam game settings offer hide and block removal while sync is enabled", async () => {
  const settings = writable({ data: { syncSteamGames: true } });
  const calls = [];
  globalThis.steamGameActions = { settings, calls };
  const stub = async source => dataModule(await createModuleLoader({}).resolveImports(compile(source, { generate: "client" }).js.code, "stub.svelte"));
  const empty = await stub("<div></div>");
  const mocks = {
    "src/lib/i18n": dataModule(`import { writable } from "${import.meta.resolve("svelte/store")}"; export const language=writable("it"); export const t=value=>value;`),
    "src/lib/stores/settings": dataModule("export const settings = globalThis.steamGameActions.settings;"),
    "src/lib/stores/games": dataModule("export const uninstallGame=async()=>{}; export const deleteGame = async id => globalThis.steamGameActions.calls.push(['remove', id]); export const hideSteamGame = async id => globalThis.steamGameActions.calls.push(['hide', id]); export const saveGame = async () => {};"),
    "src/lib/features/library/SteamGameSearch.svelte": empty,
  };
  const { load } = createModuleLoader(mocks);
  const Panel = (await import(await load("src/lib/features/library/GameDetailsPanel.svelte"))).default;
  const game = { id: "steam-game", name: "Portal", steamAppId: 400, steamInstallPath: "/steam/Portal", executablePath: null, automaticName: "Portal", nameOverride: null };
  const target = document.createElement("div"); document.body.append(target);
  const instance = mount(Panel, { target, props: { game, section: "danger", onRemoved: () => calls.push(["closed"]) } });
  try {
    await settle();
    const button = label => [...target.querySelectorAll("button")].find(item => item.textContent.trim() === label);
    assert.equal(button("Disinstalla gioco"), undefined);
    assert.equal(button("Rimuovi gioco").disabled, true);
    assert.equal(button("Nascondi dalla libreria").disabled, false);
    settings.set({ data: { syncSteamGames: false } }); await settle();
    assert.equal(button("Rimuovi gioco").disabled, false);
    button("Nascondi dalla libreria").click(); await settle();
    assert.deepEqual(calls, [["hide", game.id], ["closed"]]);
  } finally { await unmount(instance); target.remove(); delete globalThis.steamGameActions; }
});
