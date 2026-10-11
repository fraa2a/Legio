import assert from "node:assert/strict";
import { test } from "node:test";
import { createModuleLoader, dataModule } from "./test_module_loader.mjs";
const { load } = createModuleLoader({ "@tauri-apps/api/core": dataModule("export const invoke = (...args) => globalThis.workInvoke(...args);") });
test("releasing a consumer cancels exactly once", async () => {
  const calls = []; globalThis.workInvoke = async (...args) => { calls.push(args); };
  const { createArtworkWork } = await import(await load("src/lib/services/artwork-work.ts"));
  const work = createArtworkWork(42, "visible"); work.release(); work.release();
  assert.deepEqual(calls, [["cancel_artwork_request", { requestId: work.requestId }]]);
});
