import { readFile } from "node:fs/promises";
import { compile } from "svelte/compiler";
import ts from "typescript";

export const dataModule = (code) => `data:text/javascript;base64,${Buffer.from(code).toString("base64")}`;

export function artworkPacket({ bytes, ...metadata }) {
  const header = new TextEncoder().encode(JSON.stringify(metadata));
  const packet = new Uint8Array(4 + header.length + bytes.length);
  new DataView(packet.buffer).setUint32(0, header.length, true);
  packet.set(header, 4);
  packet.set(bytes, 4 + header.length);
  return packet.buffer;
}

export function createModuleLoader(mocks) {
  const modules = new Map();

  async function resolveImports(code, path) {
    for (const match of [...code.matchAll(/(?:from\s*|import\s*)["']([^"']+)["']/g)]) {
      const specifier = match[1];
      const resolved = specifier.startsWith(".") ? new URL(specifier, `file:///${path}`).pathname.slice(1) : null;
      const url = resolved === null
        ? mocks[specifier] ?? import.meta.resolve(specifier)
        : await load(mocks[resolved] || resolved.endsWith(".json") || resolved.endsWith(".svelte") ? resolved : resolved + ".ts");
      code = code.replaceAll(`"${specifier}"`, JSON.stringify(url)).replaceAll(`'${specifier}'`, JSON.stringify(url));
    }
    return code;
  }

  async function load(path) {
    if (mocks[path]) return mocks[path];
    if (modules.has(path)) return modules.get(path);
    const loading = (async () => {
      const source = await readFile(new URL(`../${path}`, import.meta.url), "utf8");
      const code = path.endsWith(".svelte")
        ? compile(source, { generate: "client", filename: path }).js.code
        : ts.transpileModule(source, { compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ESNext } }).outputText;
      return dataModule(await resolveImports(code, path));
    })();
    modules.set(path, loading);
    return loading;
  }

  return { load, resolveImports };
}
