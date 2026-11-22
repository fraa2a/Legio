import assert from "node:assert/strict";
import { readdir, readFile } from "node:fs/promises";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { test } from "node:test";

// The frontend translates backend errors by their exact English text, so every message it knows must still exist in Rust.
const root = fileURLToPath(new URL("../", import.meta.url));

async function rustFiles(directory) {
  const files = [];
  for (const entry of await readdir(join(root, directory), { withFileTypes: true })) {
    const path = join(directory, entry.name);
    if (entry.isDirectory()) files.push(...await rustFiles(path));
    else if (entry.name.endsWith(".rs")) files.push(path);
  }
  return files;
}

// String literal contents, unescaped; char literals are dropped first so a `'"'` cannot open a string.
async function rustLiterals() {
  const literals = [];
  for (const path of await rustFiles("src-tauri/src")) {
    const source = (await readFile(join(root, path), "utf8")).replace(/'(?:\\.|[^'\\\n])'/g, "''");
    for (const match of source.matchAll(/"((?:[^"\\]|\\.)*)"/gs)) {
      literals.push(match[1].replace(/\\\n\s*/g, "").replace(/\\(["\\])/g, "$1").replace(/\\n/g, "\n"));
    }
  }
  return literals;
}

// The source of `const name ... = {` (or `[`) up to its closing line.
async function frontendTable(path, name, end = "\n};") {
  const source = await readFile(join(root, path), "utf8");
  const start = source.indexOf(`const ${name}`);
  assert.notEqual(start, -1, `${name} in ${path}`);
  return source.slice(start, source.indexOf(end, start));
}

function errorKeys(table) {
  return [...table.matchAll(/^\s*"((?:[^"\\]|\\.)*)":/gm)].map((match) => match[1].replace(/\\(["\\])/g, "$1"));
}

const literals = await rustLiterals();

test("translated backend errors exist verbatim in Rust", async () => {
  const keys = [
    ...errorKeys(await frontendTable("src/lib/features/modding/modding-model.ts", "knownErrors")),
    ...errorKeys(await frontendTable("src/lib/features/modding/mod-tools-model.ts", "toolErrors")),
  ];
  assert.ok(keys.length > 10);
  const missing = keys.filter((key) => !literals.some((literal) => literal.includes(key)));
  assert.deepEqual(missing, [], "frontend error keys without a Rust message");
});

test("translated backend error patterns match a Rust format template", async () => {
  const table = await frontendTable("src/lib/features/modding/modding-model.ts", "knownPatterns", "\n];");
  const patterns = [...table.matchAll(/^\s*\[\/(.+)\/([a-z]*), "/gm)].map((match) => new RegExp(match[1], match[2]));
  assert.ok(patterns.length > 3);
  // Placeholders like `{name}` or `{}` stand for the text the pattern captures.
  const templates = literals.filter((literal) => /\{[\w.:?]*\}/.test(literal)).map((literal) => literal.replace(/\{[\w.:?]*\}/g, "Sample"));
  const unmatched = patterns.filter((pattern) => !templates.some((template) => pattern.test(template)));
  assert.deepEqual(unmatched.map(String), [], "frontend error patterns without a Rust template");
});
