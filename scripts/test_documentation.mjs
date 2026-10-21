import assert from "node:assert/strict";
import { readdir, readFile, access } from "node:fs/promises";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { test } from "node:test";

const root = fileURLToPath(new URL("../", import.meta.url));

async function markdownFiles(directory) {
  const files = [];
  for (const entry of await readdir(join(root, directory), { withFileTypes: true })) {
    const path = join(directory, entry.name);
    if (entry.isDirectory()) files.push(...await markdownFiles(path));
    else if (entry.name.endsWith(".md")) files.push(path);
  }
  return files;
}

test("documentation links and images reference existing local paths", async () => {
  const files = [
    "README.md", "CONTRIBUTING.md", "SECURITY.md", "THIRD_PARTY_NOTICES.md",
    "scripts/README.md", "tools/linux-rendering/README.md",
    ...await markdownFiles("_meta"),
    ...await markdownFiles("packaging"),
  ];
  const missing = [];
  for (const path of files) {
    const markdown = await readFile(join(root, path), "utf8");
    for (const match of markdown.matchAll(/\]\(([^\s)]+)(?:\s+"[^"]*")?\)/g)) {
      const target = match[1];
      if (/^(?:[a-z][a-z\d+.-]*:|#|\/\/)/i.test(target)) continue;
      const local = decodeURIComponent(target.split(/[?#]/)[0]);
      if (!local) continue;
      try {
        await access(resolve(root, dirname(path), local));
      } catch {
        missing.push(`${path}: ${target}`);
      }
    }
  }
  assert.deepEqual(missing, [], `Missing documentation targets:\n${missing.join("\n")}`);
});
