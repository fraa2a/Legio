import assert from "node:assert/strict";
import { test } from "node:test";
import { createModuleLoader, dataModule } from "./test_module_loader.mjs";
const { load } = createModuleLoader({ "src/lib/i18n": dataModule("export const t = s => s;") });

test("environment maps preserve own properties and reject duplicate special keys", async () => {
  const { parseMap } = await import(await load("src/lib/services/launch-fields.ts"));
  const result = parseMap("__proto__=value\nconstructor=other", "Environment");
  assert.equal(Object.hasOwn(result, "__proto__"), true);
  assert.equal(result.__proto__, "value");
  assert.equal(result.constructor, "other");
  assert.throws(() => parseMap("__proto__=first\n__proto__=second", "Environment"));
});

test("closing a draft waits for its current save then flushes the latest revision", async () => {
  const { createSaveQueue } = await import(await load("src/lib/services/save-queue.ts"));
  const queue = createSaveQueue();
  const calls = [];
  let done;
  const pending = queue.run(async () => { calls.push("first"); await new Promise(resolve => { done = resolve; }); });
  assert.equal(queue.run(async () => { throw new Error("must coalesce"); }), pending);
  const closing = queue.flush(async () => { calls.push("latest"); });
  assert.deepEqual(calls, ["first"]);
  done();
  await closing;
  assert.deepEqual(calls, ["first", "latest"]);
  await assert.rejects(queue.run(async () => { throw new Error("failed"); }), /failed/);
  await queue.run(async () => { calls.push("retry"); });
  assert.equal(calls.at(-1), "retry");
});
