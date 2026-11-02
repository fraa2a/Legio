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
  packageLoader: "bepinex", supported: true, binding: null, steamMatchId: null, ...extra,
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

test("the Thunderstore game can only be chosen without an automatic Steam match", () => {
  const bound = ecosystemGameId => ({ gameId: "g", ecosystemGameId, activeProfileId: null });
  assert.equal(model.canChooseEcosystemGame(moddable("a", "Manual")), true);
  assert.equal(model.canChooseEcosystemGame(moddable("b", "Steam", { steamAppId: 1, steamMatchId: "lethal-company" })), false);
  assert.equal(model.canChooseEcosystemGame(moddable("c", "Bound to the match", { steamMatchId: "lethal-company", binding: bound("lethal-company") })), false);
  assert.equal(model.canChooseEcosystemGame(moddable("d", "Manual override", { steamMatchId: "lethal-company", binding: bound("riskofrain2") })), true);
  assert.equal(model.canChooseEcosystemGame(moddable("e", "Manual binding", { binding: bound("riskofrain2") })), true);
});

test("catalog download failures are translated", () => {
  assert.equal(model.describeModdingError("The mod catalog could not be downloaded: offline", "it"), "Impossibile scaricare il catalogo delle mod: offline");
  assert.equal(
    model.describeModdingError("The mod catalog of this game is not available yet. Try again in a moment", "it"),
    "Il catalogo delle mod di questo gioco non è ancora disponibile. Riprova tra poco.",
  );
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

test("the mod page lists profiles only and shows the active one, else the first", () => {
  const profiles = [{ id: "b", gameId: "g", name: "beta", createdAt: 0 }, { id: "a", gameId: "g", name: "Alpha", createdAt: 0 }];
  const game = (activeProfileId) => moddable("g", "G", { binding: { gameId: "g", ecosystemGameId: "lethal-company", activeProfileId } });
  assert.deepEqual(model.profileChoices(profiles), [{ value: "a", label: "Alpha" }, { value: "b", label: "beta" }]);
  assert.deepEqual(model.shownProfile(game("b"), profiles), { profile: profiles[0], active: true });
  assert.deepEqual(model.shownProfile(game(null), profiles), { profile: profiles[1], active: false });
  assert.equal(model.shownProfile(game(null), []), null);
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
  assert.deepEqual(model.packageQuery({ ...model.defaultBrowserFilters, categories: ["Items"] }, 0).categories, ["Items"]);
  const progress = { profileId: "p", package: null, stage: "downloading", current: 2, total: 4, downloadedBytes: 50, totalBytes: 200 };
  assert.equal(model.installPercent(progress), 25);
  assert.equal(model.installPercent({ ...progress, stage: "extracting" }), 25);
  assert.equal(model.installPercent({ ...progress, stage: "resolving", current: 0, total: 0 }), 0);
});

const installedMod = (fullName, extra = {}) => ({
  profileId: "p", fullName, version: "1.0.0", enabled: true, onlineSource: true, installedAt: 0, installOrder: 0, isLoader: false, dependencies: [], ...extra,
});

test("dependents, error mapping and game search", () => {
  const mods = [
    installedMod("A-Core"),
    installedMod("B-Addon", { dependencies: ["a-core", "BepInEx-BepInExPack"] }),
    installedMod("C-Other"),
  ];
  assert.deepEqual(model.dependentsOf("a-core", mods), ["B-Addon"]);
  assert.deepEqual(model.dependentsOf("C-Other", mods), []);
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
  assert.equal(
    model.describeModdingError("These mods could not be found, so the profile was not changed: A-B-1.0.0, C-D-2.0.0 (+3)", "it"),
    "Il profilo non è stato modificato perché queste mod non sono state trovate: A-B-1.0.0, C-D-2.0.0 (+3)",
  );
  assert.equal(model.describeModdingError("The link is not a web address", "it"), "Il collegamento non è un indirizzo web.");
  assert.deepEqual(model.classifyModdingError(new Error("Download cancelled."), "it"), { kind: "cancelled", message: "Installazione annullata." });
  assert.deepEqual(model.classifyModdingError("Disk full", "it"), { kind: "error", message: "Disk full" });
  const games = [{ id: "lethal-company", displayName: "Lethal Company" }, { id: "valheim", displayName: "Valheim" }];
  assert.deepEqual(model.filterEcosystemGames(games, "LETHAL").map(game => game.id), ["lethal-company"]);
  assert.equal(model.filterEcosystemGames(games, " ").length, 2);
});

test("installed mods are searched, sorted and filtered with the loader kept on top", () => {
  const mods = [
    installedMod("BepInEx-BepInExPack", { isLoader: true, installOrder: 0, installedAt: 50 }),
    installedMod("Zed-Apple_Pie", { installOrder: 1, installedAt: 30 }),
    installedMod("Amy-Mango", { installOrder: 2, installedAt: 10, enabled: false }),
    installedMod("Bob-Éclair", { installOrder: 3, installedAt: 20 }),
    installedMod("Cat-Kiwi", { installOrder: 4, installedAt: 40, enabled: false }),
  ];
  const names = filters => model.arrangeInstalledMods(mods, { ...model.defaultInstalledFilters, ...filters }, new Set(["cat-kiwi"])).map(mod => mod.fullName.split("-")[1]);
  assert.deepEqual(names({}), ["BepInExPack", "Apple_Pie", "Mango", "Éclair", "Kiwi"]);
  assert.deepEqual(names({ descending: true }), ["BepInExPack", "Kiwi", "Éclair", "Mango", "Apple_Pie"]);
  assert.deepEqual(names({ sort: "name" }), ["BepInExPack", "Apple_Pie", "Éclair", "Kiwi", "Mango"]);
  assert.deepEqual(names({ sort: "name", descending: true }), ["BepInExPack", "Mango", "Kiwi", "Éclair", "Apple_Pie"]);
  assert.deepEqual(names({ sort: "date" }), ["BepInExPack", "Mango", "Éclair", "Apple_Pie", "Kiwi"]);
  assert.deepEqual(names({ disabled: "first" }), ["BepInExPack", "Mango", "Kiwi", "Apple_Pie", "Éclair"]);
  assert.deepEqual(names({ disabled: "last" }), ["BepInExPack", "Apple_Pie", "Éclair", "Mango", "Kiwi"]);
  assert.deepEqual(names({ disabled: "last", sort: "name", descending: true }), ["BepInExPack", "Éclair", "Apple_Pie", "Mango", "Kiwi"]);
  assert.deepEqual(names({ text: "apple pie" }), ["Apple_Pie"]);
  assert.deepEqual(names({ text: "ECLAIR" }), ["Éclair"], "accents and case are ignored");
  assert.deepEqual(names({ text: "amy" }), ["Mango"], "the owner is searched too");
  assert.deepEqual(names({ text: "pack" }), ["BepInExPack"]);
  assert.deepEqual(names({ onlyUpdates: true }), ["Kiwi"]);
  assert.deepEqual(names({ text: "zzz" }), []);
  assert.equal(model.hasInstalledFilters({ ...model.defaultInstalledFilters, text: " " }), false);
  assert.equal(model.hasInstalledFilters({ ...model.defaultInstalledFilters, onlyUpdates: true }), true);
  assert.equal(model.installedSortOptions("it").length, 3);
  assert.equal(model.disabledPositionOptions("it").length, 3);
});

test("dependency health, enable plans and dependents of installed mods", () => {
  const mods = [
    installedMod("BepInEx-BepInExPack", { isLoader: true }),
    installedMod("A-Core", { dependencies: ["BepInEx-BepInExPack"] }),
    installedMod("B-Lib", { enabled: false, dependencies: ["A-Core", "Gone-Missing"] }),
    installedMod("C-Mid", { enabled: false, dependencies: ["b-lib"] }),
    installedMod("D-Top", { dependencies: ["C-Mid", "A-Core", "D-Top"] }),
    installedMod("E-Local", { onlineSource: false }),
    installedMod("F-Off", { enabled: false, dependencies: ["C-Mid"] }),
  ];
  const issues = model.dependencyIssues(mods);
  assert.deepEqual([...issues.keys()].sort(), ["b-lib", "d-top"]);
  assert.deepEqual(issues.get("b-lib"), { missing: ["Gone-Missing"], disabled: [] }, "a disabled mod is only checked for missing packages");
  assert.deepEqual(issues.get("d-top"), { missing: [], disabled: ["C-Mid"] }, "its own name is ignored");
  assert.equal(issues.has("e-local"), false, "mods without known dependencies have no indicator");
  assert.deepEqual(model.dependencyIssueLabels(issues.get("b-lib"), "it"), ["Dipendenza mancante: Missing"]);
  assert.deepEqual(model.dependencyIssueLabels(issues.get("d-top"), "it"), ["Dipendenza disattivata: Mid"]);

  assert.deepEqual(model.enablePlan(["C-Mid"], mods), ["B-Lib", "C-Mid"], "dependencies come first");
  assert.deepEqual(model.enablePlan(["D-Top"], mods), ["B-Lib", "C-Mid"], "enabled mods are not listed");
  assert.deepEqual(model.enablePlan(["A-Core"], mods), []);
  assert.deepEqual(model.dependencyFix(issues.get("d-top"), mods), { install: [], enable: ["B-Lib", "C-Mid"] });
  assert.deepEqual(model.dependencyFix(issues.get("b-lib"), mods), { install: ["Gone-Missing"], enable: [] });

  assert.deepEqual(model.enabledDependents("A-Core", mods), ["D-Top"], "disabled mods in between do not pass the need on");
  assert.deepEqual(model.enabledDependents("c-mid", mods), ["D-Top"]);
  const chain = [installedMod("X-Base"), installedMod("Y-Mid", { dependencies: ["X-Base"] }), installedMod("Z-Top", { dependencies: ["Y-Mid"] })];
  assert.deepEqual(model.enabledDependents("X-Base", chain), ["Z-Top", "Y-Mid"], "the most dependent mods are switched off first");
  assert.deepEqual(model.enabledDependents("Z-Top", chain), []);
  const cycle = [installedMod("P-One", { dependencies: ["P-Two"], enabled: false }), installedMod("P-Two", { dependencies: ["P-One"], enabled: false })];
  assert.deepEqual(model.enablePlan(["P-One"], cycle), ["P-Two", "P-One"]);
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
    if (command === "list_profile_mods") return [{ fullName: "X-One", dependencies: [] }, { fullName: "X-Two", dependencies: [] }];
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
    { profileId: "p", fullName: "BepInEx-BepInExPack", version: "5.4.0", enabled: true, onlineSource: true, installedAt: 0, installOrder: 0, isLoader: true, dependencies: [] },
    { profileId: "p", fullName: "A-Core", version: "1.0.0", enabled: true, onlineSource: true, installedAt: 0, installOrder: 1, isLoader: false, dependencies: [] },
    { profileId: "p", fullName: "B-Addon", version: "1.0.0", enabled: true, onlineSource: true, installedAt: 0, installOrder: 2, isLoader: false, dependencies: ["A-Core"] },
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
    gameId: "g", ecosystemGameId: "lethal-company", profile: { id: "p", gameId: "g", name: "Main", createdAt: 0 }, locked: false, busy: false, tabs: { tabs: [], active: null, recent: [] }, onTabsChange() {}, onBrowse() {},
  });
  try {
    const target = view.target;
    assert.ok(target.textContent.includes("3 installate · 3 attive"));
    const toggles = [...target.querySelectorAll('li input[type="checkbox"]')];
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

async function installedFixture(initial, calls, extra = {}) {
  let mods = initial;
  const { load } = await fixture("installedExtraInvoke", async (command, args) => {
    calls.push([command, args]);
    if (command === "list_profile_mods") return mods;
    if (command === "list_mod_updates") return [];
    if (command === "get_mod_launch_status") return null;
    if (command === "get_mod_package") return null;
    if (command === "set_profile_mod_enabled") {
      mods = mods.map(mod => mod.fullName === args.fullName ? { ...mod, enabled: args.enabled } : mod);
      return undefined;
    }
    if (command === "set_all_profile_mods_enabled") {
      mods = mods.map(mod => mod.isLoader ? mod : { ...mod, enabled: args.enabled });
      return extra.bulk ?? { changed: 2, failed: [] };
    }
    if (command === "install_mods") return { installed: ["Gone-Missing-1.0.0"], skipped: [], warnings: [] };
    throw new Error(`unexpected ${command}`);
  });
  return () => render(load, "src/lib/features/modding/InstalledModsList.svelte", {
    gameId: "g", ecosystemGameId: "lethal-company", profile: { id: "p", gameId: "g", name: "Main", createdAt: 0 }, locked: false, busy: false, tabs: { tabs: [], active: null, recent: [] }, onTabsChange() {}, onBrowse() {},
  });
}

const titles = target => [...target.querySelectorAll("li span.truncate.font-medium")].map(node => node.textContent.trim());

test("installed mods are searched and sorted from the list header", async () => {
  const calls = [];
  const open = await installedFixture([
    installedMod("BepInEx-BepInExPack", { isLoader: true }),
    installedMod("Zed-Apple_Pie", { installOrder: 1 }),
    installedMod("Amy-Mango", { installOrder: 2, enabled: false }),
  ], calls);
  const view = await open();
  try {
    const target = view.target;
    assert.deepEqual(titles(target), ["BepInExPack", "Apple Pie", "Mango"]);
    const search = target.querySelector('input[type="search"]');
    search.value = "mang"; search.dispatchEvent(new Event("input", { bubbles: true })); await settle();
    assert.deepEqual(titles(target), ["Mango"]);
    search.value = "zzz"; search.dispatchEvent(new Event("input", { bubbles: true })); await settle();
    assert.ok(target.textContent.includes("Nessuna mod corrisponde ai filtri"));
    button(target, "Azzera i filtri").click(); await settle();
    assert.deepEqual(titles(target), ["BepInExPack", "Apple Pie", "Mango"]);
    button(target, "Ordine crescente").click(); await settle();
    assert.deepEqual(titles(target), ["BepInExPack", "Mango", "Apple Pie"]);
  } finally { await view.destroy(); delete globalThis.installedExtraInvoke; }
});

test("installed mods show dependency warnings with a one-click fix", async () => {
  const calls = [];
  const open = await installedFixture([
    installedMod("BepInEx-BepInExPack", { isLoader: true }),
    installedMod("A-Lib", { installOrder: 1, enabled: false }),
    installedMod("B-Top", { installOrder: 2, dependencies: ["A-Lib", "Gone-Missing"] }),
  ], calls);
  const view = await open();
  try {
    const target = view.target;
    assert.ok(target.textContent.includes("Dipendenza mancante: Missing"));
    assert.ok(target.textContent.includes("Dipendenza disattivata: Lib"));
    button(target, "Risolvi").click(); await settle();
    assert.deepEqual(calls.filter(([command]) => command === "set_profile_mod_enabled"), [["set_profile_mod_enabled", { profileId: "p", fullName: "A-Lib", enabled: true }]]);
    assert.deepEqual(calls.find(([command]) => command === "install_mods"), ["install_mods", { profileId: "p", packages: [{ fullName: "Gone-Missing" }] }]);
  } finally { await view.destroy(); delete globalThis.installedExtraInvoke; }
});

test("enabling a mod enables its dependencies and disabling asks about dependents", async () => {
  const calls = [];
  const open = await installedFixture([
    installedMod("A-Lib", { installOrder: 1, enabled: false }),
    installedMod("B-Mid", { installOrder: 2, enabled: false, dependencies: ["A-Lib"] }),
    installedMod("C-Top", { installOrder: 3, dependencies: ["B-Mid"] }),
    installedMod("D-Solo", { installOrder: 4, enabled: false }),
  ], calls);
  const view = await open();
  const switches = () => calls.filter(([command]) => command === "set_profile_mod_enabled").map(([, args]) => `${args.fullName}:${args.enabled}`);
  try {
    const target = view.target;
    const toggles = () => [...target.querySelectorAll('li input[type="checkbox"]')];
    toggles()[1].click(); await settle();
    assert.deepEqual(switches(), ["A-Lib:true", "B-Mid:true"], "dependencies are enabled first");
    calls.length = 0;
    toggles()[0].click(); await settle();
    const dialog = target.querySelector('[role="dialog"]');
    assert.ok(dialog.textContent.includes("Top"), "the dialog lists the enabled dependents");
    assert.deepEqual(switches(), [], "nothing changes before the choice");
    button(dialog, "Disattiva solo questa").click(); await settle();
    assert.deepEqual(switches(), ["A-Lib:false"]);
    calls.length = 0;
    toggles()[1].click(); await settle();
    button(target.querySelector('[role="dialog"]'), "Disattiva anche le dipendenti").click(); await settle();
    assert.deepEqual(switches(), ["C-Top:false", "B-Mid:false"]);
    calls.length = 0;
    toggles()[3].click(); await settle();
    assert.deepEqual(switches(), ["D-Solo:true"], "mods nobody needs switch without asking");
  } finally { await view.destroy(); delete globalThis.installedExtraInvoke; }
});

test("enable all and disable all report the mods that failed", async () => {
  const calls = [];
  const open = await installedFixture([
    installedMod("BepInEx-BepInExPack", { isLoader: true }),
    installedMod("A-Lib", { installOrder: 1 }),
    installedMod("B-Top", { installOrder: 2, enabled: false }),
  ], calls, { bulk: { changed: 1, failed: [{ fullName: "B-Top", message: "Stop the game before changing its installation" }] } });
  const view = await open();
  try {
    const target = view.target;
    button(target, "Disattiva tutte").click(); await settle();
    assert.deepEqual(calls.find(([command]) => command === "set_all_profile_mods_enabled"), ["set_all_profile_mods_enabled", { profileId: "p", enabled: false }]);
    assert.ok(target.textContent.includes("Mod disattivate: 1. Alcune non sono state disattivate:"));
    assert.ok(target.textContent.includes("Top: Chiudi il gioco prima di modificarne l'installazione."));
    assert.equal(button(target, "Disattiva tutte").disabled, true, "nothing is left enabled");
    assert.equal(button(target, "Attiva tutte").disabled, false);
  } finally { await view.destroy(); delete globalThis.installedExtraInvoke; }
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
    tabs: { tabs: [], active: null, recent: [] }, onTabsChange() {},
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
    assert.ok(view.target.textContent.includes("1 risultati"));
    assert.ok(button(view.target, "Aggiorna il catalogo"), "the catalog is refreshed from an icon button");
    assert.ok(!view.target.textContent.includes("pacchetti nel catalogo"));
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
    if (command === "get_mod_profile_size") return 0;
    throw new Error(`unexpected ${command}`);
  }, {
    // Lucide icons make the inlined module tree of the whole mod page too large for the heap.
    "src/lib/components/ui/Icon.svelte": await stub("<span></span>"),
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

test("the title bar gets the profile controls and an inactive profile is not activated on its own", async () => {
  const calls = [];
  const profiles = [{ id: "p2", gameId: "a", name: "Zeta", createdAt: 0 }, { id: "p1", gameId: "a", name: "Main", createdAt: 0 }];
  const binding = (activeProfileId) => ({ gameId: "a", ecosystemGameId: "lethal-company", activeProfileId });
  const { load } = await fixture("titleInvoke", async (command, args) => {
    calls.push([command, args]);
    if (command === "list_moddable_games") return [moddable("a", "Alpha", { binding: binding(null) })];
    if (command === "list_mod_profiles") return profiles;
    if (command === "list_games" || command === "list_profile_mods" || command === "list_mod_updates") return [];
    if (command === "refresh_mod_catalog") return { ecosystemGameId: "lethal-company", packageCount: 1, syncedAt: 1 };
    if (command === "get_mod_launch_status") {
      return { bound: true, ecosystemGameId: "lethal-company", activeProfileId: null, launchSupported: true, loaderDeployed: false, preloaderFound: false, doorstopVersion: null };
    }
    if (command === "get_mod_profile_size") return 2048;
    if (command === "set_active_mod_profile") return binding(args.profileId);
    throw new Error(`unexpected ${command}`);
  }, {
    "src/lib/components/ui/Icon.svelte": await stub("<span></span>"),
    "src/lib/services/dialog": dataModule("export const pickModArchive = async () => null, pickModProfileFile = async () => null, pickModProfileExportPath = async () => null;"),
  });
  const navigation = await import(await load("src/lib/stores/navigation.ts"));
  const titleBar = await import(await load("src/lib/stores/title-bar.ts"));
  navigation.openModManager("a", "game");
  const view = await render(load, "src/lib/features/modding/GameModsView.svelte", { gameId: "a" });
  const host = await stub("<script>let { content } = $props();</script>{@render content.start?.()}{@render content.end?.()}");
  const target = document.createElement("div");
  document.body.append(target);
  let bar = null;
  try {
    const content = get(titleBar.titleBarContent);
    assert.equal(content.title, "Main", "the first profile by name is shown");
    bar = mount((await import(host)).default, { target, props: { content } });
    await settle();
    // jsdom has no popover or Web Animations API.
    dom.window.HTMLElement.prototype.showPopover ??= () => {};
    dom.window.HTMLElement.prototype.animate ??= () => ({ set onfinish(callback) { callback(); } });
    target.querySelector("#mod-active-profile").click(); await settle();
    assert.deepEqual([...target.querySelectorAll('[role="option"]')].map(option => option.textContent.trim()), ["Main", "Zeta"]);
    assert.ok(button(target, "Nuovo profilo") && button(target, "Elimina profilo"));
    assert.ok(view.target.textContent.includes("Profilo non attivo"));
    assert.ok(view.target.textContent.includes("2,0 KB"));
    assert.deepEqual(calls.filter(([command]) => command === "get_mod_profile_size")[0], ["get_mod_profile_size", { profileId: "p1" }]);
    assert.equal(calls.some(([command]) => command === "set_active_mod_profile"), false);
    button(view.target, "Attiva profilo").click(); await settle();
    assert.deepEqual(calls.find(([command]) => command === "set_active_mod_profile"), ["set_active_mod_profile", { gameId: "a", profileId: "p1" }]);
  } finally {
    if (bar !== null) await unmount(bar);
    target.remove();
    await view.destroy();
    delete globalThis.titleInvoke;
  }
  assert.equal(get(titleBar.titleBarContent), null, "the title bar is cleared on unmount");
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
