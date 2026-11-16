import assert from "node:assert/strict";
import { test } from "node:test";
import { readFile } from "node:fs/promises";
import { createModuleLoader, dataModule } from "./test_module_loader.mjs";

const english = JSON.parse(await readFile(new URL("../src/lib/i18n/en.json", import.meta.url), "utf8"));
const { load } = createModuleLoader({
  "src/lib/i18n": dataModule(`const english = ${JSON.stringify(english)}; export const t = (value, _language, values = []) => (english[value] ?? value).replace(/\\{(\\d+)\\}/g, (_, index) => String(values[Number(index)] ?? ""));`),
});
const model = await import(await load("src/lib/features/library/launch-profiles.ts"));

const inherit = { arguments: null, environment: null, runnerPath: null, linuxPerformance: null };
const profile = (id, gameId, name, overrides = {}) => ({ id, gameId, name, overrides: { ...inherit, ...overrides } });
const state = {
  profiles: [profile("a", "g1", "Vanilla"), profile("b", "g1", "Modded", { arguments: ["-mods"] }), profile("c", "g2", "Window")],
  lastUsed: { g1: "b", g2: "gone", g3: "a" },
};

test("profiles are listed per game and the last used one must still belong to it", () => {
  assert.deepEqual(model.profilesOf(state, "g1").map((entry) => entry.id), ["a", "b"]);
  assert.deepEqual(model.profilesOf(state, "none"), []);
  assert.equal(model.lastUsedProfileId(state, "g1"), "b");
  assert.equal(model.lastUsedProfileId(state, "g2"), null, "a deleted profile falls back to the base settings");
  assert.equal(model.lastUsedProfileId(state, "g3"), null, "another game's profile is ignored");
  assert.equal(model.lastUsedProfileId(state, "g4"), null);
});

test("the menu starts with the base settings and names the selected profile", () => {
  const profiles = model.profilesOf(state, "g1");
  assert.deepEqual(model.profileMenuOptions(profiles, "Default"), [
    { id: null, name: "Default" },
    { id: "a", name: "Vanilla" },
    { id: "b", name: "Modded" },
  ]);
  assert.equal(model.profileName(profiles, "b", "Default"), "Modded");
  assert.equal(model.profileName(profiles, null, "Default"), "Default");
});

test("names follow the backend rules", () => {
  const profiles = model.profilesOf(state, "g1");
  assert.equal(model.profileNameError("Window", profiles, null), null);
  assert.equal(model.profileNameError("  Window  ", profiles, null), null);
  assert.notEqual(model.profileNameError("", profiles, null), null);
  assert.notEqual(model.profileNameError("   ", profiles, null), null);
  assert.notEqual(model.profileNameError("x".repeat(model.maxProfileNameLength + 1), profiles, null), null);
  assert.equal(model.profileNameError("é".repeat(model.maxProfileNameLength), profiles, null), null);
  assert.notEqual(model.profileNameError("a\nb", profiles, null), null);
  assert.notEqual(model.profileNameError("vanilla", profiles, null), null, "case is ignored");
  assert.equal(model.profileNameError("vanilla", profiles, "a"), null, "a profile may keep its own name");
  assert.equal(model.profileNameError("Vanilla", model.profilesOf(state, "g2"), null), null, "names are unique per game");
});

test("a copy gets a free name within the length limit", () => {
  const profiles = model.profilesOf(state, "g1");
  assert.equal(model.copyName("Vanilla", profiles), "Vanilla (copy)");
  const crowded = [...profiles, profile("d", "g1", "Vanilla (copy)")];
  assert.equal(model.copyName("Vanilla", crowded), "Vanilla (copy 2)");
  const long = "x".repeat(model.maxProfileNameLength);
  const copy = model.copyName(long, [profile("e", "g1", long)]);
  assert.equal([...copy].length, model.maxProfileNameLength);
  assert.ok(copy.endsWith(" (copy)"));
});

test("a new draft inherits everything and produces no overrides", () => {
  const draft = model.draftFromProfile(null);
  assert.equal(draft.name, "");
  assert.deepEqual(model.overridesFromDraft(draft), inherit);
  assert.equal(model.overrideCount(inherit), 0);
});

test("a draft round-trips every override and keeps inheriting fields null", () => {
  const stored = profile("p", "g1", "Full", {
    arguments: ["-windowed", "two words"],
    environment: { MODE: "window" },
    runnerPath: "/runners/ge",
    linuxPerformance: { gameMode: true, gamescope: { width: 1920, height: 1080, fps: null } },
  });
  const draft = model.draftFromProfile(stored);
  assert.equal(draft.argumentsText, '-windowed "two words"');
  assert.deepEqual(model.overridesFromDraft(draft), stored.overrides);
  assert.equal(model.overrideCount(stored.overrides), 4);

  draft.argumentsOverridden = false;
  draft.environmentOverridden = false;
  const partial = model.overridesFromDraft(draft);
  assert.equal(partial.arguments, null);
  assert.equal(partial.environment, null);
  assert.equal(partial.runnerPath, "/runners/ge");
  assert.equal(model.overrideCount(partial), 2);
});

test("an overridden field may be empty and clears the game's value", () => {
  const draft = model.draftFromProfile(null);
  draft.argumentsOverridden = true;
  draft.environmentOverridden = true;
  assert.deepEqual(model.overridesFromDraft(draft), { ...inherit, arguments: [], environment: {} });
});

test("bad input is reported instead of being dropped", () => {
  const draft = model.draftFromProfile(null);
  draft.environmentOverridden = true;
  draft.environmentText = "no separator";
  assert.throws(() => model.overridesFromDraft(draft));
  draft.environmentOverridden = false;
  draft.argumentsOverridden = true;
  draft.argumentsText = '"unterminated';
  assert.throws(() => model.overridesFromDraft(draft));
  draft.argumentsOverridden = false;
  draft.runnerOverridden = true;
  assert.throws(() => model.overridesFromDraft(draft), /runner/i);
});
