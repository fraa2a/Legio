import assert from "node:assert/strict";
import { test } from "node:test";
import { JSDOM } from "jsdom";
import { compile } from "svelte/compiler";
import { createModuleLoader, dataModule } from "./test_module_loader.mjs";

const dom = new JSDOM("<!doctype html><html><body></body></html>", { pretendToBeVisual: true });
for (const key of ["window", "document", "navigator", "HTMLElement", "HTMLInputElement", "Element", "Node", "Text", "Comment", "Event", "MutationObserver", "SubmitEvent", "HTMLMediaElement"]) {
  Object.defineProperty(globalThis, key, { configurable: true, value: dom.window[key] });
}
const { mount, unmount, flushSync } = await import("svelte");
const { get } = await import("svelte/store");
globalThis.matchMedia = () => ({ matches: false });
const settle = async () => { for (let i = 0; i < 6; i++) { await new Promise(resolve => setImmediate(resolve)); flushSync(); } };
const wait = ms => new Promise(resolve => setTimeout(resolve, ms));

const storeImport = `import { writable } from ${JSON.stringify(import.meta.resolve("svelte/store"))};`;
const i18n = dataModule(`${storeImport} export const language = writable("it"); export const t = (value, _language, values = []) => value.replace(/\\{(\\d+)\\}/g, (_, index) => String(values[Number(index)] ?? ""));`);
const stub = async source => dataModule(await createModuleLoader({}).resolveImports(compile(source, { generate: "client" }).js.code, "stub.svelte"));

async function fixture(name, invoke, extra = {}) {
  globalThis[name] = invoke;
  return createModuleLoader({
    "src/lib/i18n": i18n,
    "src/lib/services/invoke": dataModule(`export const invoke = (...args) => globalThis.${name}(...args);`),
    "@tauri-apps/api/event": dataModule("export const listen = async () => () => {};"),
    "src/lib/stores/launch": dataModule(`${storeImport} export const launchStateByGame = writable(new Map());`),
    "src/lib/services/dialog": dataModule("export const pickModArchive = async () => globalThis.moddingArchive ?? null;"),
    "src/lib/components/ui/Dialog.svelte": await stub('<script>let { open, title, children, onClose } = $props();</script>{#if open}<section role="dialog" aria-label={title}>{@render children(onClose)}</section>{/if}'),
    "src/lib/components/ui/ArtworkTile.svelte": await stub("<script>let { children } = $props();</script><li>{@render children(true)}</li>"),
    "src/lib/features/modding/ModIcon.svelte": await stub("<span></span>"),
    ...extra,
  });
}

async function render(load, path, props = {}) {
  const Component = (await import(await load(path))).default;
  const target = document.createElement("div");
  document.body.append(target);
  const instance = mount(Component, { target, props });
  await settle();
  return { target, destroy: async () => { await unmount(instance); target.remove(); } };
}

const button = (target, label) => [...target.querySelectorAll("button")].find(item => item.textContent.trim() === label || item.getAttribute("aria-label") === label);

const modelLoader = createModuleLoader({ "src/lib/i18n": i18n });
const model = await import(await modelLoader.load("src/lib/features/modding/modding-model.ts"));

const moddable = (gameId, name, extra = {}) => ({
  gameId, name, steamAppId: null, ecosystemGameId: "lethal-company", displayName: "Lethal Company",
  packageLoader: "bepinex", supported: true, binding: null, ...extra,
});
const summary = (fullName, extra = {}) => {
  const [owner, name] = fullName.split("-");
  return {
    fullName, owner, name, packageUrl: `https://thunderstore.io/c/lc/p/${owner}/${name}/`, categories: [], isDeprecated: false,
    isPinned: false, hasNsfwContent: false, ratingScore: 1, dateCreated: "2024-01-01", dateUpdated: "2024-01-02",
    totalDownloads: 10, latestVersion: "1.0.0", description: "", icon: "", ...extra,
  };
};

test("moddable games list supported and set up games first", () => {
  const games = [
    moddable("c", "Zeta", { ecosystemGameId: null, displayName: null, packageLoader: null, supported: false }),
    moddable("b", "Beta", { supported: false, packageLoader: "melonloader" }),
    moddable("a", "Alpha"),
    moddable("d", "Delta", { binding: { gameId: "d", ecosystemGameId: "lethal-company", activeProfileId: null } }),
  ];
  assert.deepEqual(model.sortModdableGames(games).map(game => game.gameId), ["d", "a", "b", "c"]);
  assert.deepEqual(games.map(model.moddableGameState), ["unmatched", "unsupported", "ready", "ready"]);
});

test("profile names follow the backend rules", () => {
  const profiles = [{ id: "1", gameId: "g", name: "Default", createdAt: 0 }, { id: "2", gameId: "g", name: "Default (copia)", createdAt: 0 }];
  assert.equal(model.profileNameError("  ", profiles, "it"), "Inserisci un nome.");
  assert.equal(model.profileNameError("x".repeat(65), profiles, "it"), "Il nome può avere al massimo 64 caratteri.");
  assert.equal(model.profileNameError("a/b", profiles, "it"), "Il nome non può contenere / o \\.");
  assert.equal(model.profileNameError("a\tb", profiles, "it"), "Il nome non può contenere / o \\.");
  assert.equal(model.profileNameError("default", profiles, "it"), "Esiste già un profilo con questo nome.");
  assert.equal(model.profileNameError("DEFAULT", profiles, "it", "1"), null, "renaming keeps its own name");
  assert.equal(model.copyProfileName("Default", profiles, "it"), "Default (copia 2)");
  assert.deepEqual(model.profileOptions(profiles, "it").map(option => option.value), ["", "1", "2"]);
});

test("package ids and installed state", () => {
  assert.deepEqual(model.splitPackageId("BepInEx-BepInExPack-5.4.2100"), { fullName: "BepInEx-BepInExPack", version: "5.4.2100" });
  assert.equal(model.displayPackageId("Owner-More_Suits-1.2.0"), "More Suits 1.2.0");
  const installed = model.indexByFullName([{ fullName: "Owner-Mod", version: "1.0.0" }]);
  const updates = model.indexByFullName([{ fullName: "owner-mod", installedVersion: "1.0.0", latestVersion: "2.0.0" }]);
  assert.deepEqual(model.installedState("OWNER-mod", installed, updates), { kind: "outdated", version: "1.0.0", latestVersion: "2.0.0" });
  assert.deepEqual(model.installedState("Owner-Other", installed, updates), { kind: "missing" });
});

test("launch notices, queries and progress", () => {
  const status = { bound: true, ecosystemGameId: "lc", activeProfileId: "p", launchSupported: true, loaderDeployed: false, preloaderFound: false, doorstopVersion: null };
  assert.equal(model.launchNotice(null), "none");
  assert.equal(model.launchNotice({ ...status, activeProfileId: null }), "none");
  assert.equal(model.launchNotice({ ...status, launchSupported: false }), "launch-unsupported");
  assert.equal(model.launchNotice(status), "loader-missing");
  // BepInEx in the profile but not yet copied to the game folder, as before the first launch.
  assert.equal(model.launchNotice({ ...status, preloaderFound: true }), "none");
  assert.equal(model.launchNotice({ ...status, loaderDeployed: true }), "loader-missing");
  assert.deepEqual(model.packageQuery({ ...model.defaultBrowserFilters, text: "  suits " }, 30), {
    text: "suits", includeDeprecated: false, includeNsfw: false, sort: "mostDownloads", offset: 30, limit: 30,
  });
  assert.deepEqual(model.packageQuery({ ...model.defaultBrowserFilters, category: "Items" }, 0).categories, ["Items"]);
  const progress = { profileId: "p", package: null, stage: "downloading", current: 2, total: 4, downloadedBytes: 50, totalBytes: 200 };
  assert.equal(model.installPercent(progress), 25);
  assert.equal(model.installPercent({ ...progress, stage: "extracting" }), 25);
  assert.equal(model.installPercent({ ...progress, stage: "resolving", current: 0, total: 0 }), 0);
});

test("dependents, error mapping and game search", () => {
  const mods = [{ fullName: "A-Core", version: "1.0.0" }, { fullName: "B-Addon", version: "2.0.0" }, { fullName: "C-Other", version: "1.0.0" }];
  const details = {
    "b-addon": { versions: [{ versionNumber: "2.0.0", dependencies: ["A-Core-1.0.0", "BepInEx-BepInExPack-5.4.0"] }] },
    "c-other": { versions: [{ versionNumber: "1.0.0", dependencies: [] }] },
  };
  assert.deepEqual(model.dependentsOf("a-core", mods, details), ["B-Addon"]);
  assert.equal(model.describeModdingError("Stop the game before changing its installation", "it"), "Chiudi il gioco prima di modificarne l'installazione.");
  assert.equal(model.describeModdingError("Something else", "it"), "Something else");
  assert.equal(
    model.describeModdingError("The active mod profile has no BepInEx loader. Install BepInExPack in the profile first.", "it"),
    "Il profilo mod attivo non ha BepInEx. Installalo dal gestore delle mod o scegli Vanilla per avviare senza mod.",
  );
  assert.equal(
    model.describeModdingError("winhttp.dll already exists in the game folder and was not installed by Legio.", "it"),
    "winhttp.dll è già nella cartella del gioco e non è stato installato da Legio. Spostalo o rimuovilo per avviare con le mod.",
  );
  assert.equal(model.describeModdingError('A mod profile named "Main" already exists', "it"), 'Esiste già un profilo chiamato "Main". Scegli un altro nome.');
  assert.equal(
    model.describeModdingError("Mods are being changed in the active mod profile. Wait for it to finish and launch again.", "it"),
    "Le mod del profilo attivo sono in fase di modifica. Attendi il termine e avvia di nuovo.",
  );
  assert.equal(
    model.describeModdingError("The mod loader of this game is not in the mod catalog", "it"),
    "Il loader di mod di questo gioco non è presente nel catalogo mod.",
  );
  assert.deepEqual(model.classifyModdingError(new Error("Download cancelled."), "it"), { kind: "cancelled", message: "Installazione annullata." });
  assert.deepEqual(model.classifyModdingError("Disk full", "it"), { kind: "error", message: "Disk full" });
  const games = [{ id: "lethal-company", displayName: "Lethal Company" }, { id: "valheim", displayName: "Valheim" }];
  assert.deepEqual(model.filterEcosystemGames(games, "LETHAL").map(game => game.id), ["lethal-company"]);
  assert.equal(model.filterEcosystemGames(games, " ").length, 2);
});

test("the Mod page marks unsupported and unmatched games and opens the manager", async () => {
  const calls = [];
  const games = [
    moddable("c", "Zeta", { ecosystemGameId: null, displayName: null, packageLoader: null, supported: false }),
    moddable("b", "Beta", { supported: false, packageLoader: "melonloader", displayName: "Beta TS" }),
    moddable("a", "Alpha", { binding: { gameId: "a", ecosystemGameId: "lethal-company", activeProfileId: "p1" } }),
  ];
  const { load } = await fixture("moddingPageInvoke", async (command, args) => {
    calls.push([command, args]);
    if (command === "list_moddable_games") return games;
    if (command === "list_mod_profiles") return [{ id: "p1", gameId: "a", name: "Main", createdAt: 0 }];
    if (command === "list_profile_mods") return [{ fullName: "X-One" }, { fullName: "X-Two" }];
    if (command === "list_ecosystem_games") return [{ id: "lethal-company", displayName: "Lethal Company", packageLoader: "bepinex", supported: true }];
    throw new Error(`unexpected ${command}`);
  });
  const navigation = await import(await load("src/lib/stores/navigation.ts"));
  const page = await render(load, "src/lib/features/modding/ModdingPage.svelte");
  try {
    const text = page.target.textContent;
    assert.ok(text.indexOf("Giochi con mod") < text.indexOf("Altri giochi"));
    assert.ok(text.includes("Main · 2 mod"));
    assert.ok(text.includes("Loader non supportato"));
    assert.ok(text.includes("Il loader di Beta TS non è ancora supportato"));
    assert.ok(text.includes("Da collegare"));
    button(page.target, "Gestisci le mod di Alpha").click();
    assert.deepEqual(get(navigation.selectedModGame), { gameId: "a", returnTo: "mods" });
    assert.equal(get(navigation.activeSection), "mods");
    button(page.target, "Collega Zeta a Thunderstore").click(); await settle();
    assert.ok(page.target.querySelector('[role="dialog"]'));
    assert.ok(calls.some(([command]) => command === "list_ecosystem_games"));
    assert.ok(page.target.textContent.includes("Lethal Company"));
  } finally { await page.destroy(); delete globalThis.moddingPageInvoke; }
});

test("the Mod page shows empty and error states", async () => {
  let response = async () => [];
  const { load } = await fixture("moddingPageStates", (...args) => response(...args));
  const empty = await render(load, "src/lib/features/modding/ModdingPage.svelte");
  try { assert.ok(empty.target.textContent.includes("Nessun gioco in libreria")); } finally { await empty.destroy(); }
  response = async () => { throw new Error("Thunderstore unreachable"); };
  const failed = await render(load, "src/lib/features/modding/ModdingPage.svelte");
  try { assert.ok(failed.target.textContent.includes("Thunderstore unreachable")); } finally { await failed.destroy(); delete globalThis.moddingPageStates; }
});

test("installed mods toggle, update and uninstall with a dependents warning", async () => {
  const calls = [];
  let mods = [
    { profileId: "p", fullName: "BepInEx-BepInExPack", version: "5.4.0", enabled: true, onlineSource: true, installedAt: 0, installOrder: 0, isLoader: true },
    { profileId: "p", fullName: "A-Core", version: "1.0.0", enabled: true, onlineSource: true, installedAt: 0, installOrder: 1, isLoader: false },
    { profileId: "p", fullName: "B-Addon", version: "1.0.0", enabled: true, onlineSource: true, installedAt: 0, installOrder: 2, isLoader: false },
  ];
  const { load } = await fixture("installedInvoke", async (command, args) => {
    calls.push([command, args]);
    if (command === "list_profile_mods") return mods;
    if (command === "list_mod_updates") return [{ fullName: "A-Core", installedVersion: "1.0.0", latestVersion: "1.1.0" }];
    if (command === "get_mod_launch_status") return null;
    if (command === "get_mod_package") {
      const versions = args.fullName === "B-Addon" ? [{ versionNumber: "1.0.0", dependencies: ["A-Core-1.0.0"] }] : [{ versionNumber: "1.0.0", dependencies: [] }];
      return { ...summary(args.fullName), versions };
    }
    if (command === "set_profile_mod_enabled") return undefined;
    if (command === "update_profile_mods") return { installed: ["A-Core-1.1.0"], skipped: [], warnings: [] };
    if (command === "uninstall_profile_mod") { mods = mods.filter(mod => mod.fullName !== args.fullName); return { dependents: ["B-Addon"] }; }
    throw new Error(`unexpected ${command}`);
  });
  const view = await render(load, "src/lib/features/modding/InstalledModsList.svelte", {
    gameId: "g", ecosystemGameId: "lethal-company", profile: { id: "p", gameId: "g", name: "Main", createdAt: 0 }, locked: false, busy: false, onBrowse() {},
  });
  try {
    const target = view.target;
    assert.ok(target.textContent.includes("3 installate · 3 attive"));
    const toggles = [...target.querySelectorAll('input[type="checkbox"]')];
    assert.equal(toggles[0].disabled, true, "the loader cannot be disabled");
    toggles[2].click(); await settle();
    assert.deepEqual(calls.find(([command]) => command === "set_profile_mod_enabled"), ["set_profile_mod_enabled", { profileId: "p", fullName: "B-Addon", enabled: false }]);
    button(target, "Aggiorna").click(); await settle();
    assert.deepEqual(calls.find(([command]) => command === "update_profile_mods"), ["update_profile_mods", { profileId: "p", fullNames: ["A-Core"] }]);
    button(target, "Disinstalla Core").click(); await settle();
    const dialog = target.querySelector('[role="dialog"]');
    assert.ok(dialog.textContent.includes("Addon"), "the confirmation lists installed dependents");
    button(dialog, "Disinstalla").click(); await settle();
    assert.deepEqual(calls.find(([command]) => command === "uninstall_profile_mod"), ["uninstall_profile_mod", { profileId: "p", fullName: "A-Core" }]);
    assert.ok(target.textContent.includes("Core disinstallata. Queste mod dipendono da essa"));
  } finally { await view.destroy(); delete globalThis.installedInvoke; }
});

test("the browser debounces search and installs from results", async () => {
  const calls = [];
  const { load } = await fixture("browserInvoke", async (command, args) => {
    calls.push([command, args]);
    if (command === "search_mods") return { total: 1, items: [summary("Owner-More_Suits")] };
    if (command === "list_mod_categories") return ["Items", "Suits"];
    if (command === "list_profile_mods" || command === "list_mod_updates") return [];
    if (command === "get_mod_launch_status") return null;
    if (command === "install_mods") return { installed: ["Owner-More_Suits-1.0.0"], skipped: [], warnings: [] };
    throw new Error(`unexpected ${command}`);
  });
  const view = await render(load, "src/lib/features/modding/ModBrowser.svelte", {
    gameId: "g", ecosystemGameId: "lethal-company", profile: { id: "p", gameId: "g", name: "Main", createdAt: 0 }, disabled: false,
  });
  try {
    await wait(350); await settle();
    const input = view.target.querySelector("#mod-search");
    for (const value of ["s", "su", "suits"]) {
      input.value = value;
      input.dispatchEvent(new dom.window.Event("input", { bubbles: true }));
      await settle();
    }
    await wait(350); await settle();
    const searches = calls.filter(([command]) => command === "search_mods");
    assert.deepEqual(searches.at(-1), ["search_mods", {
      ecosystemGameId: "lethal-company",
      query: { text: "suits", includeDeprecated: false, includeNsfw: false, sort: "mostDownloads", offset: 0, limit: 30 },
    }]);
    assert.equal(searches.length, 2, "typing triggers one search after the initial one");
    assert.ok(view.target.textContent.includes("More Suits"));
    button(view.target, "Installa").click(); await settle();
    assert.deepEqual(calls.find(([command]) => command === "install_mods"), ["install_mods", { profileId: "p", packages: [{ fullName: "Owner-More_Suits" }] }]);
  } finally { await view.destroy(); delete globalThis.browserInvoke; }
});

test("rebinding a game with profiles asks for confirmation first", async () => {
  const { load } = await fixture("rebindInvoke", async (command) => {
    if (command === "list_ecosystem_games") {
      return [
        { id: "lethal-company", displayName: "Lethal Company", packageLoader: "bepinex", supported: true },
        { id: "valheim", displayName: "Valheim", packageLoader: "bepinex", supported: true },
      ];
    }
    throw new Error(`unexpected ${command}`);
  });
  const selected = [];
  const props = { open: true, gameName: "Alpha", currentId: "lethal-company", profileCount: 2, onClose() {}, onSelect: async (id) => { selected.push(id); } };
  const view = await render(load, "src/lib/features/modding/EcosystemGameDialog.svelte", props);
  try {
    const valheim = button(view.target, "Valheim");
    assert.equal(valheim.getAttribute("aria-pressed"), "false");
    valheim.click(); await settle();
    assert.equal(valheim.getAttribute("aria-pressed"), "true");
    button(view.target, "Collega").click(); await settle();
    const confirm = view.target.querySelector('[aria-label="Cambiare gioco Thunderstore?"]');
    assert.ok(confirm.textContent.includes("I profili di Alpha restano collegati"));
    assert.deepEqual(selected, [], "nothing changes before the confirmation");
    button(confirm, "Cambia gioco").click(); await settle();
    assert.deepEqual(selected, ["valheim"]);
  } finally { await view.destroy(); }

  const fresh = await render(load, "src/lib/features/modding/EcosystemGameDialog.svelte", { ...props, profileCount: 0 });
  try {
    button(fresh.target, "Valheim").click(); await settle();
    button(fresh.target, "Collega").click(); await settle();
    assert.deepEqual(selected, ["valheim", "valheim"], "without profiles the game is rebound right away");
  } finally { await fresh.destroy(); delete globalThis.rebindInvoke; }
});

test("a new profile is activated and gets BepInEx right away", async () => {
  const calls = [];
  let profiles = [];
  const binding = (activeProfileId) => ({ gameId: "a", ecosystemGameId: "lethal-company", activeProfileId });
  const { load } = await fixture("createInvoke", async (command, args) => {
    calls.push([command, args]);
    if (command === "list_moddable_games") return [moddable("a", "Alpha", { binding: binding(null) })];
    if (command === "list_mod_profiles") return profiles;
    if (command === "list_games" || command === "list_profile_mods" || command === "list_mod_updates" || command === "list_mod_categories") return [];
    if (command === "refresh_mod_catalog") return { ecosystemGameId: "lethal-company", packageCount: 1, syncedAt: 1 };
    if (command === "get_mod_launch_status") {
      return { bound: true, ecosystemGameId: "lethal-company", activeProfileId: null, launchSupported: true, loaderDeployed: false, preloaderFound: false, doorstopVersion: null };
    }
    if (command === "create_mod_profile") { profiles = [{ id: "p1", gameId: "a", name: args.name, createdAt: 0 }]; return profiles[0]; }
    if (command === "set_active_mod_profile") return binding(args.profileId);
    if (command === "install_mod_loader") return { installed: ["BepInEx-BepInExPack-5.4.0"], skipped: [], warnings: [] };
    if (command === "search_mods") return { total: 0, items: [] };
    throw new Error(`unexpected ${command}`);
  }, {
    "src/lib/features/library/GameIcon.svelte": await stub("<span></span>"),
    "src/lib/services/dialog": dataModule("export const pickModArchive = async () => null, pickModProfileFile = async () => null, pickModProfileExportPath = async () => null;"),
  });
  const view = await render(load, "src/lib/features/modding/GameModsView.svelte", { gameId: "a" });
  try {
    button(view.target, "Crea profilo").click(); await settle();
    const input = view.target.querySelector("#mod-profile-name");
    input.value = "Main";
    input.dispatchEvent(new dom.window.Event("input", { bubbles: true }));
    await settle();
    view.target.querySelector("form").dispatchEvent(new dom.window.Event("submit", { bubbles: true, cancelable: true }));
    await settle(); await settle();
    const order = calls.map(([command]) => command);
    assert.ok(order.indexOf("create_mod_profile") < order.indexOf("set_active_mod_profile"));
    assert.deepEqual(calls.find(([command]) => command === "install_mod_loader"), ["install_mod_loader", { profileId: "p1" }]);
    assert.ok(order.indexOf("set_active_mod_profile") < order.indexOf("install_mod_loader"));
  } finally { await view.destroy(); delete globalThis.createInvoke; }
});

test("a leave guard holds navigation until it proceeds", async () => {
  const { load } = createModuleLoader({});
  const navigation = await import(await load("src/lib/stores/navigation.ts"));
  navigation.openModManager("g", "game");
  let pending = null;
  const release = navigation.setLeaveGuard((proceed) => { pending = proceed; return true; });
  navigation.closeModManager();
  assert.deepEqual(get(navigation.selectedModGame), { gameId: "g", returnTo: "game" });
  pending();
  assert.equal(get(navigation.selectedGameId), "g");
  assert.equal(get(navigation.activeSection), "library");
  navigation.selectSection("mods");
  assert.equal(get(navigation.activeSection), "mods", "the guard is released after proceeding");
  release();
});
