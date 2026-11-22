import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { test } from "node:test";

// Settings defaults and limits are checked by the Rust tests that read settings-contract.json.
const read = path => readFile(new URL(`../${path}`, import.meta.url), "utf8");
const quoted = text => new Set([...text.matchAll(/"([^"]+)"/g)].map(match => match[1]));
const union = (source, name) => quoted(source.match(new RegExp(`export type ${name} = ([^;]+);`))[1]);
const variants = (source, name) => new Set([...source.match(new RegExp(`pub enum ${name} \\{([^}]+)\\}`))[1].matchAll(/^\s*(\w+),/gm)]
  .map(([, variant]) => variant[0].toLowerCase() + variant.slice(1)));

test("settings choices match the backend", async () => {
  const [state, catalog, database] = await Promise.all([
    read("src/lib/services/local-state.ts"), read("src/lib/services/catalog.ts"), read("src-tauri/src/database.rs"),
  ]);
  assert.deepEqual(union(state, "Theme"), quoted(database.match(/fn as_str\(&self\) -> &'static str \{([\s\S]+?)\n {4}\}/)[1]));
  assert.deepEqual(union(state, "LibraryViewMode"), variants(database, "LibraryViewMode"));
  assert.deepEqual(union(state, "LibrarySortKey"), variants(database, "LibrarySortKey"));
  assert.deepEqual(union(catalog, "CatalogSort"), quoted(database.match(/settings\.store_sort\.as_str\(\),([^)]+)\)/)[1]));
  assert.deepEqual(union(catalog, "CatalogAvailability"), quoted(database.match(/settings\.store_availability\.as_str\(\),([^)]+)\)/)[1]));
});
