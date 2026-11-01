import assert from "node:assert/strict";
import { test } from "node:test";
import { JSDOM } from "jsdom";
import { compile } from "svelte/compiler";
import { createModuleLoader, dataModule } from "./test_module_loader.mjs";

const dom = new JSDOM("<!doctype html><html><body></body></html>", { pretendToBeVisual: true });
for (const key of [
  "window", "document", "navigator", "Node", "Text", "Comment", "Element", "HTMLElement", "HTMLInputElement", "HTMLTextAreaElement",
  "HTMLButtonElement", "HTMLMediaElement", "Event", "KeyboardEvent", "MouseEvent", "MutationObserver",
]) {
  Object.defineProperty(globalThis, key, { configurable: true, value: dom.window[key] });
}
HTMLElement.prototype.showPopover = function () {};
globalThis.matchMedia = () => ({ matches: false });
const { mount, unmount, flushSync } = await import("svelte");
const settle = async () => { for (let i = 0; i < 6; i++) { await new Promise((resolve) => setImmediate(resolve)); flushSync(); } };

const storeImport = `import { writable } from "${import.meta.resolve("svelte/store")}";`;
const stub = async (source) => dataModule(await createModuleLoader({}).resolveImports(compile(source, { generate: "client" }).js.code, "stub.svelte"));
const mocks = {
  "src/lib/i18n": dataModule(storeImport + 'export const language=writable("it"); export const t=(m,l,v=[])=>m.replace(/\\{(\\d+)\\}/g,(_,i)=>String(v[i]??""));'),
  "src/lib/services/invoke": dataModule("export const invoke=(...args)=>globalThis.extraInvoke(...args);"),
  "src/lib/stores/toast": dataModule("export const showToast=(...args)=>globalThis.extraToasts.push(args);"),
  "@tauri-apps/api/event": dataModule("export const listen=async(name,handler)=>{globalThis.extraListeners.set(name,handler);return()=>globalThis.extraListeners.delete(name);};"),
  "@tauri-apps/plugin-dialog": dataModule("export const open=async(...args)=>globalThis.extraDialog.open(...args); export const save=async(...args)=>globalThis.extraDialog.save(...args);"),
  "src/lib/components/ui/Dialog.svelte": await stub('<script>let {open,children,onClose}=$props();</script>{#if open}<div role="dialog">{@render children(onClose)}</div>{/if}'),
};
const { load } = createModuleLoader(mocks);

function reset(invoke) {
  globalThis.extraInvoke = async (command, args) => { globalThis.extraCalls.push([command, args]); return invoke(command, args); };
  globalThis.extraCalls = [];
  globalThis.extraToasts = [];
  globalThis.extraListeners = new Map();
  globalThis.extraDialog = { open: async () => null, save: async () => null };
}

const buttonWith = (target, label) => [...target.querySelectorAll("button")].find((button) => button.textContent.trim() === label);
const typeInto = (input, value) => { input.value = value; input.dispatchEvent(new Event("input", { bubbles: true })); };

async function mountComponent(path, props) {
  const Component = (await import(await load(path))).default;
  const target = document.createElement("div");
  document.body.append(target);
  const instance = mount(Component, { target, props });
  await settle();
  return { target, dispose: async () => { await unmount(instance); target.remove(); } };
}

test("config model chooses controls, validates numbers and collects only changed entries", async () => {
  const model = await import(await load("src/lib/features/modding/mod-config-model.ts"));
  const entry = (overrides) => ({ key: "K", value: "", description: null, settingType: null, defaultValue: null, acceptableValues: [], acceptableRange: null, line: 1, ...overrides });
  assert.equal(model.entryControl(entry({ settingType: "Boolean", value: "true" })), "boolean");
  assert.equal(model.entryControl(entry({ settingType: "Boolean", value: "maybe" })), "text");
  assert.equal(model.entryControl(entry({ settingType: "Mode", value: "A", acceptableValues: ["A", "B"] })), "choice");
  assert.equal(model.entryControl(entry({ settingType: "Mode", value: "A", acceptableValues: ["A", "B"], description: "Multiple values can be set at the same time" })), "text");
  assert.equal(model.entryControl(entry({ settingType: "Single", value: "1.5" })), "number");
  assert.equal(model.entryControl(entry({ settingType: "Single", value: "NaN" })), "text");
  assert.deepEqual(model.choiceOptions(entry({ acceptableValues: ["A", "B"] }), "Z").map((option) => option.value), ["Z", "A", "B"]);

  const ranged = entry({ settingType: "Int32", value: "5", acceptableRange: ["1", "10"] });
  assert.equal(model.validateEntry(ranged, "5"), null);
  assert.equal(model.validateEntry(ranged, "11"), "Il valore massimo è 10");
  assert.equal(model.validateEntry(ranged, "0"), "Il valore minimo è 1");
  assert.equal(model.validateEntry(ranged, "2.5"), "Inserisci un numero intero");
  assert.equal(model.validateEntry(ranged, ""), "Inserisci un numero");

  const config = { sections: [
    { name: "A", entries: [entry({ key: "x", value: "1", line: 1 }), entry({ key: "y", value: "2", line: 2 })] },
    { name: "B", entries: [entry({ key: "x", value: "3", line: 4 })] },
  ] };
  let changes = {};
  changes = model.setChange(changes, "A", config.sections[0].entries[0], "9");
  changes = model.setChange(changes, "A", config.sections[0].entries[1], "2");
  changes = model.setChange(changes, "B", config.sections[1].entries[0], "4");
  changes = model.setChange(changes, "B", config.sections[1].entries[0], "3");
  assert.deepEqual(model.collectEdits(config, changes), [{ section: "A", key: "x", value: "9", line: 1 }]);
  assert.equal(model.currentValue("A", config.sections[0].entries[0], changes), "9");
  assert.deepEqual(model.filterSections(config, "y").map((section) => section.id), ["0"]);

  // Repeated section and key names stay separate entries.
  const repeated = { sections: [
    { name: "General", entries: [entry({ key: "x", value: "1", line: 1 })] },
    { name: "General", entries: [entry({ key: "x", value: "1", line: 4 })] },
  ] };
  const edited = model.setChange({}, "General", repeated.sections[1].entries[0], "2");
  assert.equal(model.currentValue("General", repeated.sections[0].entries[0], edited), "1");
  assert.deepEqual(model.collectEdits(repeated, edited), [{ section: "General", key: "x", value: "2", line: 4 }]);
  assert.deepEqual(model.filterSections(repeated, "").map((section) => section.id), ["0", "1"]);
  assert.deepEqual(model.initialOpenSections(repeated), ["0", "1"]);

  const files = [{ relativePath: "plugins/b.cfg", size: 1, modified: 1 }, { relativePath: "a.cfg", size: 1, modified: 1 }, { relativePath: "plugins/A2.txt", size: 1, modified: 1 }];
  assert.deepEqual(model.filterConfigFiles(files, "").map((view) => view.file.relativePath), ["a.cfg", "plugins/A2.txt", "plugins/b.cfg"]);
  assert.deepEqual(model.filterConfigFiles(files, "PLUGINS").map((view) => view.name), ["A2.txt", "b.cfg"]);
  assert.equal(model.textSizeError("small"), null);
  assert.notEqual(model.textSizeError("a".repeat(model.maxConfigTextBytes + 1)), null);

  const dated = [{ relativePath: "b.cfg", size: 1, modified: 20 }, { relativePath: "a.cfg", size: 1, modified: 10 }, { relativePath: "c.cfg", size: 1, modified: 20 }];
  const order = (sort, query = "") => model.filterConfigFiles(dated, query, sort).map((view) => view.name);
  assert.deepEqual(order("name-asc"), ["a.cfg", "b.cfg", "c.cfg"]);
  assert.deepEqual(order("name-desc"), ["c.cfg", "b.cfg", "a.cfg"]);
  assert.deepEqual(order("modified-desc"), ["b.cfg", "c.cfg", "a.cfg"]);
  assert.deepEqual(order("modified-asc"), ["a.cfg", "b.cfg", "c.cfg"]);
  assert.deepEqual(order("modified-desc", "c."), ["c.cfg"]);
  assert.deepEqual(dated.map((file) => file.relativePath), ["b.cfg", "a.cfg", "c.cfg"]);
  assert.equal(model.configSortOptions().length, 4);
});

test("tools model describes cache rows and errors", async () => {
  const model = await import(await load("src/lib/features/modding/mod-tools-model.ts"));
  const rows = model.cacheRows({ packagesBytes: 1, catalogBytes: 2, iconsBytes: 3 });
  assert.deepEqual(rows.map((row) => [row.kind, row.bytes]), [["packages", 1], ["catalog", 2], ["icons", 3]]);
  assert.match(model.clearedMessage("catalog"), /Catalogo/);
  assert.match(model.describeToolsError("No BepInEx log found: launch the game with this profile at least once"), /^Nessun log trovato/);
  assert.match(model.describeToolsError("Mods are being installed or changed. Wait for it to finish and try again"), /installazione/);
  assert.equal(model.describeToolsError("Something else"), "Something else");
});

test("exchange model validates codes and file names", async () => {
  const model = await import(await load("src/lib/features/modding/mod-profile-exchange-model.ts"));
  assert.equal(model.normalizeProfileCode("  123E4567-E89B-12D3-A456-426614174000 "), "123e4567-e89b-12d3-a456-426614174000");
  assert.equal(model.normalizeProfileCode("not-a-code"), null);
  assert.equal(model.profileCodeError(""), null);
  assert.notEqual(model.profileCodeError("abc"), null);
  assert.equal(model.canStartImport("file", null, "", null), false);
  assert.equal(model.canStartImport("file", "/a.r2z", "", null), true);
  assert.equal(model.canStartImport("code", null, "bad", null), false);
  assert.equal(model.canStartImport("code", null, "123e4567-e89b-12d3-a456-426614174000", "invalid name"), false);
  assert.equal(model.canStartImport("file", "/a.r2z", "", "invalid name", "existing", "p1"), true);
  assert.equal(model.canStartImport("file", "/a.r2z", "", null, "existing", null), false);
  assert.equal(model.canStartImport("code", null, "bad", null, "existing", "p1"), false);
  const profiles = [{ id: "p1" }, { id: "p2" }];
  assert.equal(model.defaultTargetProfileId(profiles, "p2"), "p2");
  assert.equal(model.defaultTargetProfileId(profiles, "gone"), "p1");
  assert.equal(model.defaultTargetProfileId(profiles, null), "p1");
  assert.equal(model.defaultTargetProfileId([], "p1"), null);
  assert.equal(model.importTargetProfileId("existing", "p1"), "p1");
  assert.equal(model.importTargetProfileId("existing", null), undefined);
  assert.equal(model.importTargetProfileId("new", "p1"), undefined);
  assert.equal(model.exportFileName('My: "Profile"?'), "My- -Profile--.r2z");
  assert.equal(model.exportFileName("..."), "profile.r2z");
  assert.equal(model.fileNameOf("C:\\dir\\pack.r2z"), "pack.r2z");

  const summary = model.summarizeImport({ profile: {}, installed: ["a"], skippedFiles: [], missingPackages: ["m"], warnings: [] });
  assert.equal(summary.installed, 1);
});

const cfgText = "[General]\nEnabled = true\nMode = Easy\nSpeed = 1.5\nGreeting = Hi\n";
const cfgFile = {
  relativePath: "BepInEx.cfg",
  text: cfgText,
  bepinex: {
    sections: [{
      name: "General",
      entries: [
        { key: "Enabled", value: "true", description: "Turns it on", settingType: "Boolean", defaultValue: "true", acceptableValues: [], acceptableRange: null, line: 2 },
        { key: "Mode", value: "Easy", description: null, settingType: "Mode", defaultValue: "Easy", acceptableValues: ["Easy", "Hard"], acceptableRange: null, line: 3 },
        { key: "Speed", value: "1.5", description: null, settingType: "Single", defaultValue: "1", acceptableValues: [], acceptableRange: ["0", "10"], line: 4 },
        { key: "Greeting", value: "Hi", description: null, settingType: "String", defaultValue: "Hello", acceptableValues: [], acceptableRange: null, line: 5 },
      ],
    }],
  },
};
const noteFile = { relativePath: "notes.txt", text: "plain", bepinex: null };
const listing = [
  { relativePath: "BepInEx.cfg", size: 2048, modified: 1700000000 },
  { relativePath: "notes.txt", size: 5, modified: 1700000000 },
];

function configInvoke(files = listing) {
  return async (command, args) => {
    if (command === "list_mod_config_files") return files;
    if (command === "read_mod_config_file") return args.relativePath === "notes.txt" ? noteFile : cfgFile;
    return undefined;
  };
}

test("config editor saves only changed entries and resets values to their default", async () => {
  reset(configInvoke());
  const dirtyStates = [];
  const { target, dispose } = await mountComponent("src/lib/features/modding/ModConfigEditor.svelte", { gameId: "g", profileId: "p", onDirtyChange: (dirty) => dirtyStates.push(dirty) });
  try {
    assert.deepEqual(globalThis.extraCalls.slice(0, 2), [["list_mod_config_files", { gameId: "g", profileId: "p" }], ["read_mod_config_file", { gameId: "g", profileId: "p", relativePath: "BepInEx.cfg" }]]);
    assert.ok(target.textContent.includes("Turns it on"));
    assert.equal(buttonWith(target, "Salva").disabled, true);

    typeInto(target.querySelector("#mod-config-line-5"), "Hey");
    await settle();
    assert.equal(buttonWith(target, "Salva").disabled, false);
    assert.equal(dirtyStates.at(-1), true);
    target.querySelector('input[type="checkbox"]').click();
    await settle();

    const resetButtons = [...target.querySelectorAll('button[aria-label="Ripristina il valore predefinito"]')];
    assert.equal(resetButtons.filter((button) => !button.disabled).length, 3, "Greeting, Speed and the toggled Enabled differ from their defaults");
    typeInto(target.querySelector("#mod-config-line-4"), "99");
    await settle();
    assert.equal(buttonWith(target, "Salva").disabled, true, "an out of range number blocks saving");
    assert.ok(target.textContent.includes("Il valore massimo è 10"));
    typeInto(target.querySelector("#mod-config-line-4"), "1.5");
    await settle();
    assert.equal(buttonWith(target, "Salva").disabled, false);

    buttonWith(target, "Salva").click();
    await settle();
    const save = globalThis.extraCalls.find(([command]) => command === "save_mod_config_entries");
    assert.deepEqual(save[1], {
      gameId: "g", profileId: "p", relativePath: "BepInEx.cfg",
      edits: [{ section: "General", key: "Enabled", value: "false", line: 2 }, { section: "General", key: "Greeting", value: "Hey", line: 5 }],
    });
    assert.equal(globalThis.extraToasts.length, 1);
    assert.equal(dirtyStates.at(-1), false, "the file is re-read after saving");
  } finally { await dispose(); }
});

test("config editor picks acceptable values and edits raw text", async () => {
  reset(configInvoke());
  const { target, dispose } = await mountComponent("src/lib/features/modding/ModConfigEditor.svelte", { gameId: "g", profileId: "p" });
  try {
    target.querySelector("#mod-config-line-3").click();
    await settle();
    [...target.querySelectorAll('[role="option"]')].find((option) => option.textContent.trim() === "Hard").click();
    await settle();
    buttonWith(target, "Salva").click();
    await settle();
    assert.deepEqual(globalThis.extraCalls.find(([command]) => command === "save_mod_config_entries")[1].edits, [{ section: "General", key: "Mode", value: "Hard", line: 3 }]);

    target.querySelector('input[name="mod-config-mode"][value="raw"]').click();
    await settle();
    const area = target.querySelector("textarea");
    assert.equal(area.value, cfgText);
    typeInto(area, "[General]\nMode = Hard\n");
    await settle();
    buttonWith(target, "Salva").click();
    await settle();
    assert.deepEqual(globalThis.extraCalls.find(([command]) => command === "save_mod_config_text")[1], {
      gameId: "g", profileId: "p", relativePath: "BepInEx.cfg", text: "[General]\nMode = Hard\n",
    });
  } finally { await dispose(); }
});

test("config editor guards unsaved edits and shows plain files as raw text only", async () => {
  reset(configInvoke());
  const { target, dispose } = await mountComponent("src/lib/features/modding/ModConfigEditor.svelte", { gameId: "g", profileId: "p" });
  try {
    typeInto(target.querySelector("#mod-config-line-5"), "Changed");
    await settle();
    const fileButton = [...target.querySelectorAll("aside button")].find((button) => button.textContent.includes("notes.txt"));
    fileButton.click();
    await settle();
    assert.ok(target.querySelector('[role="dialog"]'), "switching with unsaved edits asks first");
    assert.equal(globalThis.extraCalls.filter(([command]) => command === "read_mod_config_file").length, 1);
    buttonWith(target, "Continua a modificare").click();
    await settle();
    assert.equal(target.querySelector('[role="dialog"]'), null);
    assert.equal(target.querySelector("#mod-config-line-5").value, "Changed");

    fileButton.click();
    await settle();
    buttonWith(target, "Scarta modifiche").click();
    await settle();
    assert.equal(globalThis.extraCalls.at(-1)[1].relativePath, "notes.txt");
    assert.equal(target.querySelector("textarea").value, "plain");
    assert.equal(target.querySelector('input[name="mod-config-mode"]'), null, "no form mode for plain files");

    const filter = target.querySelector("#mod-config-filter");
    typeInto(filter, "nothing");
    await settle();
    assert.ok(target.textContent.includes("Nessun file corrisponde al filtro."));
  } finally { await dispose(); }
});

test("config editor explains how to get config files and reports load errors", async () => {
  reset(configInvoke([]));
  const empty = await mountComponent("src/lib/features/modding/ModConfigEditor.svelte", { gameId: "g", profileId: "p" });
  try {
    assert.ok(empty.target.textContent.includes("Avvia il gioco con questo profilo"));
  } finally { await empty.dispose(); }

  reset(async () => { throw "Profile not found"; });
  const failed = await mountComponent("src/lib/features/modding/ModConfigEditor.svelte", { gameId: "g", profileId: "p" });
  try {
    assert.ok(failed.target.querySelector('[role="alert"]')?.textContent.includes("Profile not found"));
  } finally { await failed.dispose(); }
});

const code = "123e4567-e89b-12d3-a456-426614174000";
const outcome = {
  profile: { id: "new", gameId: "g", name: "Shared", createdAt: 1 },
  installed: ["A-B-1.0.0", "C-D-2.0.0"],
  skippedFiles: ["x.exe"],
  missingPackages: ["Gone-Mod-1.0.0"],
  warnings: [],
};

test("import dialog imports a code with progress, a summary and the new profile", async () => {
  let finish;
  reset((command) => (command === "cancel_mod_install" ? undefined : new Promise((resolve) => { finish = resolve; })));
  const imported = [];
  const { target, dispose } = await mountComponent("src/lib/features/modding/ProfileImportDialog.svelte", { open: true, gameId: "g", activeProfileId: null, onClose() {}, onImported: (profile) => imported.push(profile) });
  try {
    target.querySelector('input[name="mod-profile-import-source"][value="code"]').click();
    await settle();
    typeInto(target.querySelector("#mod-profile-import-code"), "bad");
    await settle();
    assert.equal(buttonWith(target, "Importa").disabled, true);
    assert.ok(target.textContent.includes("Il codice deve avere il formato"));
    typeInto(target.querySelector("#mod-profile-import-code"), ` ${code.toUpperCase()} `);
    typeInto(target.querySelector("#mod-profile-import-name"), " Shared ");
    await settle();
    buttonWith(target, "Importa").click();
    await settle();
    const [command, args] = globalThis.extraCalls.at(-1);
    assert.equal(command, "import_mod_profile_code");
    assert.deepEqual({ ...args, operationId: undefined }, { gameId: "g", code, profileName: "Shared", operationId: undefined, targetProfileId: undefined });
    assert.match(args.operationId, /^[0-9a-f-]{36}$/);
    assert.equal(buttonWith(target, "Annulla"), undefined, "nothing to cancel before the first event");

    const emit = (payload) => globalThis.extraListeners.get("modding-install-progress")({ payload });
    // An install started elsewhere into another new profile, without this import's operation id.
    emit({ profileId: "other", operationId: null, cancellable: true, package: "Z-Z-1.0.0", stage: "downloading", current: 1, total: 1, downloadedBytes: 1, totalBytes: 2 });
    await settle();
    assert.ok(!target.textContent.includes("Z Z"), "events of other operations are ignored");
    emit({ profileId: "new", operationId: args.operationId, cancellable: false, package: "A-B-1.0.0", stage: "extracting", current: 1, total: 2, downloadedBytes: null, totalBytes: null });
    await settle();
    assert.ok(target.textContent.includes("Estrazione"));
    assert.equal(buttonWith(target, "Annulla"), undefined, "extraction cannot be cancelled");
    emit({ profileId: "new", operationId: args.operationId, cancellable: true, package: "A-B-1.0.0", stage: "downloading", current: 2, total: 2, downloadedBytes: 10, totalBytes: 100 });
    await settle();
    assert.ok(target.textContent.includes("Pacchetto 2 di 2"));
    assert.equal(target.querySelector('[role="progressbar"]').getAttribute("aria-valuenow"), "10");

    buttonWith(target, "Annulla").click();
    await settle();
    assert.deepEqual(globalThis.extraCalls.at(-1), ["cancel_mod_install", { profileId: "new" }]);

    finish(outcome);
    await settle();
    assert.deepEqual(imported, [outcome.profile]);
    assert.ok(target.textContent.includes('Profilo "Shared" importato: 2 mod installate.'));
    assert.ok(target.textContent.includes("Gone-Mod-1.0.0"));
    assert.ok(target.textContent.includes("x.exe"));
    assert.equal(globalThis.extraListeners.size, 0, "the progress listener is released");
  } finally { await dispose(); }
});

test("import dialog imports a picked file and shows backend errors", async () => {
  reset(async () => { throw "The mod catalog of this game has not been downloaded yet"; });
  globalThis.extraDialog.open = async () => "C:\\profiles\\pack.r2z";
  const { target, dispose } = await mountComponent("src/lib/features/modding/ProfileImportDialog.svelte", { open: true, gameId: "g", activeProfileId: null, onClose() {}, onImported() { assert.fail("must not import"); } });
  try {
    assert.equal(buttonWith(target, "Importa").disabled, true);
    buttonWith(target, "Scegli file...").click();
    await settle();
    assert.ok(target.textContent.includes("pack.r2z"));
    buttonWith(target, "Importa").click();
    await settle();
    const [command, args] = globalThis.extraCalls.at(-1);
    assert.equal(command, "import_mod_profile_file");
    assert.equal(args.path, "C:\\profiles\\pack.r2z");
    assert.equal(args.profileName, undefined);
    assert.equal(typeof args.operationId, "string");
    assert.ok(target.querySelector('[role="alert"]').textContent.includes("catalog of this game"));
    assert.ok(buttonWith(target, "Importa"), "the form stays so the user can retry");
  } finally { await dispose(); }
});

test("share dialog exports a file, generates a code and copies it", async () => {
  reset(async (command) => (command === "share_mod_profile_code" ? code : undefined));
  const saves = [];
  globalThis.extraDialog.save = async (options) => { saves.push(options); return "C:\\out\\My Profile.r2z"; };
  const copied = [];
  Object.defineProperty(navigator, "clipboard", { configurable: true, value: { writeText: async (text) => { copied.push(text); } } });
  const profile = { id: "p", gameId: "g", name: "My: Profile", createdAt: 1 };
  const { target, dispose } = await mountComponent("src/lib/features/modding/ProfileShareDialog.svelte", { open: true, gameId: "g", profile, onClose() {} });
  try {
    assert.ok(target.textContent.includes("Le mod locali non sono incluse"));
    assert.ok(target.textContent.includes("vengono caricati su Thunderstore in modo pubblico"));
    buttonWith(target, "Esporta file...").click();
    await settle();
    assert.equal(saves[0].defaultPath, "My- Profile.r2z");
    assert.deepEqual(globalThis.extraCalls.at(-1), ["export_mod_profile_file", { gameId: "g", profileId: "p", destinationPath: "C:\\out\\My Profile.r2z" }]);
    assert.ok(target.textContent.includes("Profilo esportato in My Profile.r2z"));

    buttonWith(target, "Genera codice").click();
    await settle();
    assert.deepEqual(globalThis.extraCalls.at(-1), ["share_mod_profile_code", { gameId: "g", profileId: "p" }]);
    assert.equal(target.querySelector("input[readonly]").value, code);
    buttonWith(target, "Copia").click();
    await settle();
    assert.deepEqual(copied, [code]);
    assert.ok(buttonWith(target, "Copiato"));

    Object.defineProperty(navigator, "clipboard", { configurable: true, value: { writeText: async () => { throw new Error("denied"); } } });
    buttonWith(target, "Copiato").click();
    await settle();
    assert.ok(target.textContent.includes("Impossibile copiare negli appunti"));
  } finally { await dispose(); }
});

test("share dialog ignores a cancelled save dialog and reports export errors", async () => {
  reset(async () => { throw "Disk full"; });
  const profile = { id: "p", gameId: "g", name: "Pack", createdAt: 1 };
  const { target, dispose } = await mountComponent("src/lib/features/modding/ProfileShareDialog.svelte", { open: true, gameId: "g", profile, onClose() {} });
  try {
    buttonWith(target, "Esporta file...").click();
    await settle();
    assert.equal(globalThis.extraCalls.length, 0, "a cancelled picker exports nothing");
    assert.equal(target.querySelector('[role="alert"]'), null);

    globalThis.extraDialog.save = async () => "C:\\out\\Pack.r2z";
    buttonWith(target, "Esporta file...").click();
    await settle();
    assert.ok(target.querySelector('[role="alert"]').textContent.includes("Disk full"));
  } finally { await dispose(); }
});
