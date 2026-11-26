import assert from "node:assert/strict";
import { test } from "node:test";
import { createModuleLoader } from "./test_module_loader.mjs";

const { load } = createModuleLoader({});
const picker = await import(await load("src/lib/features/library/steamgriddb.ts"));

const view = (overrides = {}) => picker.pickerView({ hasKey: true, loading: false, error: null, games: 1, images: 3, searched: true, ...overrides });

test("SteamGridDB tabs map to the existing custom artwork slots", () => {
  assert.equal(picker.steamGridDbSlot("hero"), "banner");
  assert.equal(picker.steamGridDbSlot("grid"), "banner");
  assert.equal(picker.steamGridDbSlot("icon"), "icon");
  assert.equal(picker.steamGridDbSlot("logo"), null);
});

test("picker view follows key, loading, error and result state", () => {
  assert.equal(view(), "ready");
  assert.equal(view({ hasKey: false }), "no-key");
  assert.equal(view({ hasKey: null }), "loading");
  assert.equal(view({ loading: true }), "loading");
  assert.equal(view({ error: "x" }), "error");
  assert.equal(view({ games: 0, images: 0 }), "no-games");
  assert.equal(view({ images: 0 }), "no-images");
  assert.equal(view({ hasKey: false, error: "x" }), "no-key");
});

test("a key needs at least eight characters once trimmed", () => {
  assert.equal(picker.canSaveKey("   "), false);
  assert.equal(picker.canSaveKey("1234567"), false);
  assert.equal(picker.canSaveKey("  12345678  "), true);
});
