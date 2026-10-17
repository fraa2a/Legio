import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
const root = new URL('../', import.meta.url);
const context = vm.createContext({});
for (const path of ['src-tauri/vendor/flexsearch.bundle.min.js', 'src-tauri/src/source_search.js']) {
  vm.runInContext(fs.readFileSync(new URL(path, root), 'utf8'), context, { timeout: 1000 });
}
context.legioBuildReleaseIndex(['human resource machine steamgg repack', 'machine human resource', 'free download simulacra', 'portal stories mel', 'portal 2']);
assert.deepEqual(JSON.parse(context.legioSearchReleases('human resource machine')).sort(), [0, 1]);
assert.deepEqual(JSON.parse(context.legioSearchReleases('simulacra')), [2]);
assert.deepEqual(JSON.parse(context.legioSearchReleases('unrelated title')), []);
context.legioBuildReleaseIndex(Array.from({ length: 160 }, () => 'same game release'));
assert.equal(JSON.parse(context.legioSearchReleases('same game')).length, 160);
console.log('Vendored FlexSearch: release text, word order, missing titles and uncapped source results passed.');
