import assert from "node:assert/strict";
import { test } from "node:test";
import { createModuleLoader } from "./test_module_loader.mjs";

const { load } = createModuleLoader({});
const picker = await import(await load("src/lib/features/library/steamgriddb.ts"));
const shots = await import(await load("src/lib/features/library/local-screenshots.ts"));

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

test("local images pair entries with their loaded thumbnails and full images", () => {
  const entries = [{ id: "folder/a.png", name: "a.png" }, { id: "steam-1/b.jpg", name: "b.jpg" }];
  assert.deepEqual(shots.toLocalImages(entries, { "folder/a.png": "blob:t" }, { "steam-1/b.jpg": "blob:f" }), [
    { id: "folder/a.png", name: "a.png", thumbUrl: "blob:t", fullUrl: null },
    { id: "steam-1/b.jpg", name: "b.jpg", thumbUrl: null, fullUrl: "blob:f" },
  ]);
});

test("pagination arithmetic", () => {
  assert.equal(shots.pageCountOf(0, 12), 1);
  assert.equal(shots.pageCountOf(12, 12), 1);
  assert.equal(shots.pageCountOf(13, 12), 2);
  assert.equal(shots.pageCountOf(5, 0), 1);
  assert.equal(shots.pageOffset(1, 12), 0);
  assert.equal(shots.pageOffset(3, 12), 24);
  assert.equal(shots.pageOffset(0, 12), 0);
});

test("limited runner bounds concurrency and survives failures", async () => {
  let running = 0;
  let peak = 0;
  const done = [];
  await shots.runLimited([1, 2, 3, 4, 5, 6], 2, async (item) => {
    running++;
    peak = Math.max(peak, running);
    await new Promise((resolve) => setImmediate(resolve));
    running--;
    if (item === 3) throw new Error("boom");
    done.push(item);
  });
  assert.equal(peak, 2);
  assert.deepEqual(done.sort(), [1, 2, 4, 5, 6]);
});
