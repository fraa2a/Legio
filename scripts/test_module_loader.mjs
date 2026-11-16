import { mkdtempSync, rmSync } from "node:fs";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { basename, join, relative } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { compile } from "svelte/compiler";
import ts from "typescript";

// Packages that ship uncompiled .svelte components are compiled like project sources.
const sveltePackages = ["@lucide/svelte"];
const root = fileURLToPath(new URL("..", import.meta.url));

export const dataModule = (code) => `data:text/javascript;base64,${Buffer.from(code).toString("base64")}`;

export function artworkPacket({ bytes, ...metadata }) {
  const header = new TextEncoder().encode(JSON.stringify(metadata));
  const packet = new Uint8Array(4 + header.length + bytes.length);
  new DataView(packet.buffer).setUint32(0, header.length, true);
  packet.set(header, 4);
  packet.set(bytes, 4 + header.length);
  return packet.buffer;
}

// Compiled modules are written to files and imported by file: URL. Inlining each dependency as a
// nested data: URL duplicated shared components in every importer and grew the graph
// exponentially. Node never unloads a module, so each loader gets its own directory (a fresh
// instance per test), except package components, which do not depend on mocks and are shared.
const outputRoot = mkdtempSync(join(tmpdir(), "legio-modules-"));
process.on("exit", () => rmSync(outputRoot, { recursive: true, force: true }));
const packageModules = new Map();
let loaderCount = 0;
let fileCount = 0;

export function createModuleLoader(mocks) {
  const projectModules = new Map();
  const projectDirectory = join(outputRoot, `loader-${++loaderCount}`);

  async function resolveImports(code, path) {
    for (const match of [...code.matchAll(/(?:from\s*|import\s*|import\(\s*)["']([^"']+)["']/g)]) {
      const specifier = match[1];
      const resolved = specifier.startsWith(".") ? new URL(specifier, `file:///${path}`).pathname.slice(1) : null;
      const url = resolved === null
        ? mocks[specifier] ?? await resolvePackage(specifier)
        : await load(mocks[resolved] || /\.(json|svelte|js)$/.test(resolved) ? resolved : resolved + ".ts");
      code = code.replaceAll(`"${specifier}"`, JSON.stringify(url)).replaceAll(`'${specifier}'`, JSON.stringify(url));
    }
    return code;
  }

  async function resolvePackage(specifier) {
    const url = import.meta.resolve(specifier);
    if (!sveltePackages.some((name) => specifier === name || specifier.startsWith(`${name}/`))) return url;
    return load(relative(root, fileURLToPath(url)).replaceAll("\\", "/"));
  }

  async function load(path) {
    if (mocks[path]) return mocks[path];
    const modules = path.startsWith("node_modules/") ? packageModules : projectModules;
    if (modules.has(path)) return modules.get(path);
    const loading = (async () => {
      const source = await readFile(new URL(`../${path}`, import.meta.url), "utf8");
      const code = path.endsWith(".svelte")
        ? compile(source, { generate: "client", filename: path }).js.code
        : ts.transpileModule(source, { compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ESNext } }).outputText;
      const directory = modules === packageModules ? join(outputRoot, "packages") : projectDirectory;
      await mkdir(directory, { recursive: true });
      const file = join(directory, `${++fileCount}-${basename(path)}.mjs`);
      await writeFile(file, await resolveImports(code, path));
      return pathToFileURL(file).href;
    })();
    modules.set(path, loading);
    return loading;
  }

  return { load, resolveImports };
}
