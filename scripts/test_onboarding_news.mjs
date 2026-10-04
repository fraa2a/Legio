import assert from 'node:assert/strict';
import { test } from 'node:test';
import { readFile } from 'node:fs/promises';
import ts from 'typescript';
import { get } from 'svelte/store';

const dataModule = (code) => `data:text/javascript;base64,${Buffer.from(code).toString('base64')}`;
async function load(path, imports) {
  const source = await readFile(new URL('../' + path, import.meta.url), 'utf8');
  let code = ts.transpileModule(source, { compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ESNext } }).outputText;
  for (const [specifier, url] of Object.entries(imports)) code = code.replaceAll(`from "${specifier}"`, `from "${url}"`);
  return import(dataModule(code));
}
const errors = dataModule('export const toMessage = error => String(error);');
const resource = await load('src/lib/stores/resource.ts', { 'svelte/store': import.meta.resolve('svelte/store'), '../utils/errors': errors });
const resourceUrl = dataModule('export const createResource = globalThis.newsResource;');
globalThis.newsResource = resource.createResource;

test('onboarding completes only after successful saves and preserves settings', async () => {
  const calls = [];
  globalThis.onboardSave = async (value) => { calls.push(value); return value; };
  globalThis.compatSave = async () => { throw new Error('invalid runner'); };
  const { saveOnboarding } = await load('src/lib/services/onboarding.ts', {
    './local-state': dataModule('export const saveSettings = value => globalThis.onboardSave(value);'),
    './game-settings': dataModule('export const saveCompatibilityDefaults = value => globalThis.compatSave(value);'),
  });
  const draft = { theme: 'eggplant', onboardingComplete: false, appearance: { customThemes: ['existing'] }, language: 'en', verifyVerifiedDownloads: true };
  await assert.rejects(saveOnboarding(draft, {}), /invalid runner/);
  assert.equal(calls.length, 0);
  const saved = await saveOnboarding(draft, null);
  assert.equal(saved.onboardingComplete, true);
  assert.deepEqual(saved.appearance, draft.appearance);
  assert.equal(draft.onboardingComplete, false);
  globalThis.onboardSave = async () => { throw new Error('disk full'); };
  await assert.rejects(saveOnboarding(draft, null), /disk full/);
});

test('news displays persisted articles before remote refresh and retains them offline', async () => {
  const cached = { feed: { items: [{ id: 'saved' }] }, cachedAt: 1, warning: null };
  let resolve;
  let requests = 0;
  globalThis.newsRead = async () => cached;
  globalThis.newsRefresh = () => { requests++; return new Promise((yes) => { resolve = yes; }); };
  const { news, loadNews } = await load('src/lib/stores/news.ts', {
    'svelte/store': import.meta.resolve('svelte/store'), './resource': resourceUrl, '../utils/errors': errors,
    '../services/news': dataModule('export const getNews = () => globalThis.newsRead(); export const refreshNews = () => globalThis.newsRefresh();'),
  });
  const loading = loadNews();
  await new Promise(setImmediate);
  assert.equal(get(news).data.feed.items[0].id, 'saved');
  const duplicate = loadNews();
  assert.equal(requests, 1);
  resolve({ ...cached, feed: { items: [{ id: 'updated' }] }, cachedAt: Date.now() / 1000 });
  await Promise.all([loading, duplicate]);
  assert.equal(get(news).data.feed.items[0].id, 'updated');
  globalThis.newsRefresh = async () => { throw new Error('offline'); };
  await loadNews(true);
  assert.equal(get(news).data.feed.items[0].id, 'updated');
  assert.match(get(news).data.warning, /offline/);
});
