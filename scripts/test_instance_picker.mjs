import assert from "node:assert/strict";
import { test } from "node:test";
import { readFile } from "node:fs/promises";
import { JSDOM } from "jsdom";
import { createModuleLoader, dataModule } from "./test_module_loader.mjs";

const dom = new JSDOM("<!doctype html><html><body></body></html>", { pretendToBeVisual: true });
for (const key of ["window", "document", "navigator", "Node", "Text", "Comment", "Element", "HTMLElement", "HTMLMediaElement", "Event", "KeyboardEvent", "MouseEvent", "CustomEvent", "getComputedStyle"]) {
  Object.defineProperty(globalThis, key, { configurable: true, value: dom.window[key] });
}
const { mount, unmount, flushSync } = await import("svelte");
const english = JSON.parse(await readFile(new URL("../src/lib/i18n/en.json", import.meta.url), "utf8"));
const { load } = createModuleLoader({
  "src/lib/utils/motion": dataModule("export const reducedMotion = true;"),
  "src/lib/features/library/SteamArtwork.svelte": dataModule("export default function () {}"),
  "src/lib/i18n": dataModule(`import { writable } from ${JSON.stringify(import.meta.resolve("svelte/store"))}; export const language = writable("en"); const english = ${JSON.stringify(english)}; export const t = (value, _language, values = []) => (english[value] ?? value).replace(/\\{(\\d+)\\}/g, (_, index) => String(values[Number(index)] ?? ""));`),
});
const model = await import(await load("src/lib/features/library/instance-options.ts"));

const settle = async () => { for (let i = 0; i < 4; i++) { await new Promise(setImmediate); flushSync(); } };
const profile = (id, gameId, name) => ({ id, gameId, name, createdAt: 1 });
const moddable = (gameId, activeProfileId, overrides = {}) => ({
  gameId, name: "Valheim", steamAppId: 892970, ecosystemGameId: "valheim", displayName: "Valheim", packageLoader: "bepinex", supported: true,
  binding: { gameId, ecosystemGameId: "valheim", activeProfileId }, ...overrides,
});
const bases = [
  { id: "g1", name: "Valheim", subtitle: "Mai giocato", isSteam: true },
  { id: "g2", name: "Valheim", subtitle: "Ultima partita: ieri" },
  { id: "g3", name: "Valheim", subtitle: "Mai giocato" },
];

test("option ids encode the instance and the profile unambiguously", () => {
  assert.equal(model.instanceOptionId("g1"), "g1");
  assert.equal(model.instanceOptionId("g1", "p1"), "g1:p1");
  assert.deepEqual(model.parseInstanceOptionId("g1"), { gameId: "g1", profileId: null });
  assert.deepEqual(model.parseInstanceOptionId("g1:p1"), { gameId: "g1", profileId: "p1" });
});

test("only supported bound instances get profile entries, grouped under their instance", () => {
  const games = new Map([
    ["g1", moddable("g1", null)],
    ["g2", moddable("g2", null, { supported: false })],
    ["g3", moddable("g3", null, { binding: null })],
  ]);
  const profiles = new Map([["g1", [profile("pb", "g1", "beta"), profile("pa", "g1", "Alpha")]]]);
  const states = model.instanceModStates(["g1", "g2", "g3"], games, profiles, () => false);
  assert.deepEqual([...states.keys()], ["g1"]);
  const options = model.instanceReleaseOptions(bases, states);
  assert.deepEqual(options.map((option) => option.id), ["g1", "g1:pa", "g1:pb", "g2", "g3"]);
  assert.deepEqual(options.map((option) => option.profileOf), [undefined, "Valheim", "Valheim", undefined, undefined]);
  assert.deepEqual(options.map((option) => option.lastProfile), [undefined, false, true, undefined, undefined]);
  assert.equal(options[1].name, "Alpha");
  assert.equal(options[1].subtitle, "Mai giocato");
  assert.equal(options[1].isSteam, undefined);
});

test("the selected entry follows the active profile and falls back to the instance", () => {
  const state = (activeProfileId, profiles) => ({ profiles, activeProfileId, locked: false });
  const listed = [profile("pa", "g1", "Alpha")];
  assert.equal(model.selectedInstanceId("g1", undefined), "g1");
  assert.equal(model.selectedInstanceId("g1", state(null, listed)), "g1");
  assert.equal(model.selectedInstanceId("g1", state("pa", listed)), "g1:pa");
  assert.equal(model.selectedInstanceId("g1", state("pa", [])), "g1");
});

test("a running or busy instance can only keep its current profile", () => {
  const listed = [profile("pa", "g1", "Alpha"), profile("pb", "g1", "Beta")];
  const locked = new Map([["g1", { profiles: listed, activeProfileId: "pa", locked: true }]]);
  const options = model.instanceReleaseOptions(bases.slice(0, 1), locked);
  assert.deepEqual(options.map((option) => [option.id, option.disabled]), [["g1", true], ["g1:pa", false], ["g1:pb", true]]);
  const state = locked.get("g1");
  assert.equal(model.changesProfile({ gameId: "g1", profileId: "pa" }, state), false);
  assert.equal(model.changesProfile({ gameId: "g1", profileId: null }, state), true);
  assert.equal(model.changesProfile({ gameId: "g2", profileId: null }, undefined), false);
});

test("instances with the same name are told apart by their location", () => {
  const game = (overrides) => ({ id: "g", steamAppId: null, automaticName: null, nameOverride: null, name: "Valheim", steamInstallPath: null, steamAccountId: null, executablePath: null, installationRoot: null, ...overrides });
  assert.equal(model.instanceLocation(game({ steamInstallPath: "D:\\SteamLibrary\\steamapps\\common\\Valheim" })), "common/Valheim");
  assert.equal(model.instanceLocation(game({ executablePath: "E:\\Games\\Valheim 2\\valheim.exe" })), "Games/Valheim 2");
  assert.equal(model.instanceLocation(game({ executablePath: "/opt/games/valheim/valheim.x86_64" })), "games/valheim");
  assert.equal(model.instanceLocation(game({})), null);
  const games = [moddable("g1", null), moddable("g2", null), moddable("g3", null, { name: "Other" })];
  assert.equal(model.hasDuplicateName(games[0], games), true);
  assert.equal(model.hasDuplicateName(games[2], games), false);
});

test("the picker titles profile entries with the profile name and draws a tree connector", async () => {
  const Component = (await import(await load("src/lib/features/library/ReleasePicker.svelte"))).default;
  const target = document.createElement("div"); document.body.append(target);
  const chosen = [];
  const options = model.instanceReleaseOptions(bases.slice(0, 1), new Map([["g1", { profiles: [profile("pa", "g1", "Hardcore")], activeProfileId: "pa", locked: false }]]));
  const instance = mount(Component, { target, props: { steamAppId: null, options, selectedId: "g1:pa", onSelect: (id) => chosen.push(id), links: [], link: "auto" } });
  try {
    await settle();
    const trigger = target.querySelector("button");
    assert.match(trigger.textContent, /Hardcore/);
    assert.match(trigger.textContent, /Modded/);
    assert.match(trigger.textContent, /Valheim/);
    trigger.click(); await settle();
    const rows = [...target.querySelectorAll('[role="option"]')];
    assert.equal(rows.length, 2);
    assert.doesNotMatch(rows[0].textContent, /Modded/);
    assert.match(rows[1].textContent, /Modded/);
    assert.equal(rows[1].getAttribute("aria-selected"), "true");
    assert.match(rows[1].querySelector("span span.font-semibold").textContent, /^Hardcore$/);
    assert.equal(rows[1].querySelectorAll('[aria-hidden="true"] > span').length >= 1, true);
    assert.equal(rows[0].querySelectorAll("span.w-9").length, 0);
    rows[0].click(); await settle();
    assert.deepEqual(chosen, ["g1"]);
  } finally { await unmount(instance); target.remove(); }
});

test("disabled entries cannot be chosen and are skipped by the arrow keys", async () => {
  const Component = (await import(await load("src/lib/features/library/ReleasePicker.svelte"))).default;
  const target = document.createElement("div"); document.body.append(target);
  const chosen = [];
  const states = new Map([["g1", { profiles: [profile("pa", "g1", "A"), profile("pb", "g1", "B")], activeProfileId: "pa", locked: true }]]);
  const options = model.instanceReleaseOptions(bases.slice(0, 1), states);
  const instance = mount(Component, { target, props: { steamAppId: null, options, selectedId: "g1:pa", onSelect: (id) => chosen.push(id), links: [], link: "auto" } });
  try {
    await settle();
    target.querySelector("button").click(); await settle();
    const rows = [...target.querySelectorAll('[role="option"]')];
    assert.deepEqual(rows.map((row) => row.disabled), [true, false, true]);
    rows[0].click(); rows[2].click(); await settle();
    assert.deepEqual(chosen, []);
    document.activeElement.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowDown", bubbles: true }));
    assert.equal(document.activeElement, rows[1]);
  } finally { await unmount(instance); target.remove(); }
});
