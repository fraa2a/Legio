import assert from "node:assert/strict";
import { test } from "node:test";
import { readFileSync } from "node:fs";

const root = new URL("../src-tauri/", import.meta.url);
const read = path => readFileSync(new URL(path, root), "utf8");
const quoted = text => new Set([...text.matchAll(/"([a-z_]+)"/g)].map(match => match[1]));
const handler = read("src/lib.rs").split("tauri::generate_handler![")[1].split("]")[0];
const registered = new Set([...handler.matchAll(/\b[a-z_]+::([a-z_]+)/g)].map(match => match[1]));
const manifest = quoted(read("build.rs").split("commands(&[")[1].split("])")[0]);
const allowed = quoted(read("permissions/main.toml").split("commands.allow = [")[1].split("]")[0]);

test("every registered Tauri command has manifest and main window permission entries", () => {
  assert.deepEqual([...registered].filter(command => !manifest.has(command)), []);
  assert.deepEqual([...registered].filter(command => !allowed.has(command)), []);
  assert.deepEqual([...manifest].filter(command => !registered.has(command)), []);
  assert.deepEqual([...allowed].filter(command => !registered.has(command)), []);
});
