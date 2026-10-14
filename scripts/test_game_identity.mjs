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
const { writable, get } = await import("svelte/store");
const settle = async () => { for (let i = 0; i < 5; i++) { await new Promise(resolve => setImmediate(resolve)); flushSync(); } };
const waitForSearch = async () => { await new Promise(resolve => setTimeout(resolve, 320)); await settle(); };
const catalogGame = { steamAppId: 700, name: "Correct game", availability: "unknown" };

async function fixture(steamAppId, nameOverride = "My custom version") {
  const game = writable({ id: "manual", name: nameOverride ?? "Old game", automaticName: "Old game", nameOverride, steamAppId, steamInstallPath: null, steamAccountId: null, executablePath: "/games/start.exe", installationRoot: "/games" });
  const state = { game, calls: [], errors: [], fail: false };
  globalThis.identityFixture = state;
  const storeImport = `import { writable } from "${import.meta.resolve("svelte/store")}";`;
  const { resolveImports } = createModuleLoader({
    "src/lib/i18n": dataModule(storeImport + 'export const language=writable("it"); export const t=value=>value;'),
    "src/lib/stores/games": dataModule(`export const deleteGame=async()=>{}; export const saveGame=async input=>{
      const state=globalThis.identityFixture; state.calls.push(input);
      if(state.fail) throw new Error("Could not save identity");
      let current; state.game.subscribe(value=>current=value)();
      const updated={...current,...input,name:input.nameOverride??input.automaticName}; state.game.set(updated); return updated;
    };`),
    "src/lib/stores/manual-import": dataModule('export const detectManualGameSteamAppId=async()=>{throw new Error("Unexpected auto detection");};'),
    "src/lib/services/catalog": dataModule(`export const cachedCatalogSearch=()=>null;
      export const searchCatalog=async()=>({games:[${JSON.stringify(catalogGame)}]});
      export const refreshCatalogCached=async()=>({games:[${JSON.stringify(catalogGame)}]});`),
    "src/lib/stores/toast": dataModule('export const showToast=message=>globalThis.identityFixture.errors.push(message);'),
  });
  const code = compile('<script>import Panel from "./src/lib/features/library/GameDetailsPanel.svelte"; let { game }=$props();</script><Panel game={$game} onRemoved={()=>{}}/>', { generate: "client" }).js.code;
  const Component = (await import(dataModule(await resolveImports(code, "wrapper.svelte")))).default;
  const target = document.createElement("div");
  document.body.append(target);
  const instance = mount(Component, { target, props: { game } });
  await settle();
  return { ...state, target, close: async () => { await unmount(instance); target.remove(); delete globalThis.identityFixture; } };
}

for (const linked of [true, false]) {
  test(`manual game ${linked ? "with" : "without"} a Steam ID can select a catalog game and retain its custom name and executable`, async () => {
    const f = await fixture(linked ? 400 : null);
    try {
      const input = f.target.querySelector('input[type="search"]');
      assert.ok(input, "Steam catalog search must be available even with an assigned ID");
      input.value = "Correct";
      input.dispatchEvent(new Event("input", { bubbles: true }));
      await settle(); await waitForSearch();
      const choice = [...f.target.querySelectorAll("button")].find(button => button.textContent.includes("Correct game"));
      assert.ok(choice);
      choice.click(); await settle();
      assert.equal(f.calls.at(-1).steamAppId, 700);
      assert.equal(f.calls.at(-1).automaticName, "Correct game");
      assert.equal(f.calls.at(-1).nameOverride, "My custom version");
      assert.equal(get(f.game).executablePath, "/games/start.exe");
      assert.equal(get(f.game).installationRoot, "/games");
      assert.ok(f.target.textContent.includes("700"));
      assert.equal(input.value, "");
    } finally { await f.close(); }
  });
}

test("failed identity save retains the old ID and allows selecting the result again", async () => {
  const f = await fixture(400, null);
  try {
    const input = f.target.querySelector('input[type="search"]');
    assert.ok(input);
    input.value = "Correct"; input.dispatchEvent(new Event("input", { bubbles: true }));
    await settle(); await waitForSearch();
    const choice = () => [...f.target.querySelectorAll("button")].find(button => button.textContent.includes("Correct game"));
    globalThis.identityFixture.fail = true;
    choice().click(); await settle();
    assert.equal(get(f.game).steamAppId, 400);
    assert.ok(f.errors.includes("Could not save identity"));
    globalThis.identityFixture.fail = false;
    choice().click(); await settle();
    assert.equal(get(f.game).steamAppId, 700);
    assert.equal(get(f.game).name, "Correct game");
  } finally { await f.close(); }
});


test("changing Steam identity saves the current name draft before its debounce fires", async () => {
  const f = await fixture(400);
  try {
    const search = f.target.querySelector('input[type="search"]');
    search.value = "Correct"; search.dispatchEvent(new Event("input", { bubbles: true }));
    await settle(); await waitForSearch();
    const name = f.target.querySelector("#game-name");
    name.value = "New custom version"; name.dispatchEvent(new Event("input", { bubbles: true }));
    await settle();
    [...f.target.querySelectorAll("button")].find(button => button.textContent.includes("Correct game")).click();
    await settle();
    assert.equal(f.calls.at(-1).nameOverride, "New custom version");
    assert.equal(get(f.game).steamAppId, 700);
    await new Promise(resolve => setTimeout(resolve, 580)); await settle();
    assert.equal(f.calls.length, 1, "the pending name debounce must not restore the old ID");
  } finally { await f.close(); }
});
