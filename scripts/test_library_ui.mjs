import assert from "node:assert/strict";
import { test } from "node:test";
import { get, writable } from "svelte/store";
import { createModuleLoader, dataModule } from "./test_module_loader.mjs";

test("library view follows saved settings after startup and subsequent updates", async () => {
  const settings = writable({ status: "idle", data: { libraryPortrait: false } });
  globalThis.libraryViewSettings = settings;
  const { load } = createModuleLoader({
    "src/lib/stores/settings.ts": dataModule("export const settings = globalThis.libraryViewSettings;"),
  });
  const { libraryPortrait } = await import(await load("src/lib/stores/library-ui.ts"));
  assert.equal(get(libraryPortrait), false);
  settings.set({ status: "ready", data: { libraryPortrait: true } });
  assert.equal(get(libraryPortrait), true);
  settings.set({ status: "ready", data: { libraryPortrait: false } });
  assert.equal(get(libraryPortrait), false);
  delete globalThis.libraryViewSettings;
});
