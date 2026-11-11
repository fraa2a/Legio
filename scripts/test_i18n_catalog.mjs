import assert from "node:assert/strict";
import { readdir, readFile } from "node:fs/promises";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { test } from "node:test";

const root = fileURLToPath(new URL("../", import.meta.url));
const catalogPath = "src/lib/i18n/en.json";
const sourceExtensions = [".ts", ".svelte", ".js", ".rs", ".json"];

async function sourceFiles(directory) {
  const files = [];
  for (const entry of await readdir(join(root, directory), { withFileTypes: true })) {
    const path = join(directory, entry.name);
    if (entry.isDirectory()) files.push(...await sourceFiles(path));
    else if (path !== catalogPath && sourceExtensions.some((extension) => entry.name.endsWith(extension))) files.push(path);
  }
  return files;
}

async function readSources() {
  const paths = [...await sourceFiles("src"), ...await sourceFiles("src-tauri/src")];
  return Promise.all(paths.map(async (path) => [path, await readFile(join(root, path), "utf8")]));
}

function unescapeLiteral(text) {
  return text
    .replace(/\\u\{([0-9a-fA-F]+)\}/g, (_, hex) => String.fromCodePoint(parseInt(hex, 16)))
    .replace(/\\u([0-9a-fA-F]{4})/g, (_, hex) => String.fromCharCode(parseInt(hex, 16)))
    .replace(/\\n/g, "\n")
    .replace(/\\(["'`\\])/g, "$1");
}

test("catalog keys are used in the sources", async () => {
  const catalog = JSON.parse(await readFile(join(root, catalogPath), "utf8"));
  const haystack = (await readSources()).map(([, text]) => text).join("\n");
  const decoded = unescapeLiteral(haystack);
  const unused = Object.keys(catalog).filter((key) => {
    const trimmed = key.trim();
    return !haystack.includes(trimmed) && !decoded.includes(trimmed);
  });
  assert.deepEqual(unused, [], "unused catalog keys");
});

test("literal t() arguments exist in the catalog", async () => {
  const catalog = JSON.parse(await readFile(join(root, catalogPath), "utf8"));
  const literal = /\bt\(\s*(?:"((?:[^"\\\n]|\\.)*)"|'((?:[^'\\\n]|\\.)*)'|`((?:[^`\\$]|\\.)*)`)/g;
  const missing = new Set();
  for (const [path, text] of await readSources()) {
    if (path.endsWith(".rs")) continue;
    for (const match of text.matchAll(literal)) {
      const key = unescapeLiteral(match[1] ?? match[2] ?? match[3]).trim();
      if (key && !(key in catalog)) missing.add(`${path}: ${key}`);
    }
  }
  assert.deepEqual([...missing], [], "t() literals missing from the catalog");
});
