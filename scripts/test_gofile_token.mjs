import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
const root = new URL('../', import.meta.url);
const prelude = fs.readFileSync(new URL('src-tauri/src/gofile_probe.js', root), 'utf8');
const capture = fs.readFileSync(new URL('src-tauri/src/gofile_capture.js', root), 'utf8');
const fixture = fs.readFileSync(new URL('scripts/fixtures/gofile-wt.js', root), 'utf8');
function probe(script) {
  const context = vm.createContext({});
  for (const source of [prelude, script]) vm.runInContext(source, context, { timeout: 1000 });
  return vm.runInContext(capture, context, { timeout: 1000 });
}
assert.match(probe(fixture), /^HydraGofileUserAgent::HydraGofileLanguage::HydraGofileToken::\d+::secret::suffix$/);
assert.throws(() => probe('while(true) {}'), /timed out/);
assert.throws(() => probe("require('node:fs')"), /require is not defined/);
console.log('GoFile website-token probe: fixture, timeout and absent host APIs passed.');
