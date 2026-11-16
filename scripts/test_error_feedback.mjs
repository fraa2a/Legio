import assert from "node:assert/strict";
import { test } from "node:test";
import { JSDOM } from "jsdom";
import { createModuleLoader, dataModule } from "./test_module_loader.mjs";

const dom = new JSDOM(
  '<!doctype html><body><div id="plain"><span id="outside">x</span></div><p data-selectable><b id="selectable">y</b></p><input id="field"><textarea id="area"></textarea><div contenteditable="true"><i id="editable">z</i></div><div contenteditable="false" id="locked"></div></body>',
  { pretendToBeVisual: true },
);
for (const key of ["window", "document", "navigator", "HTMLElement", "HTMLDialogElement", "HTMLMediaElement", "Element", "Node", "Text", "Comment", "Event", "MouseEvent", "CustomEvent", "MutationObserver", "getComputedStyle"]) {
  Object.defineProperty(globalThis, key, { configurable: true, value: dom.window[key] });
}
const { mount, unmount, flushSync } = await import("svelte");
const storeImport = `import { writable } from "${import.meta.resolve("svelte/store")}";`;
const mocks = {
  "src/lib/i18n": dataModule(storeImport + 'export const language = writable("it"); export const t = (value, _language, args = []) => value.replace(/\\{(\\d+)\\}/g, (_, index) => args[index]);'),
};
const { load } = createModuleLoader(mocks);
const { allowsNativeContextMenu } = await import(await load("src/lib/utils/context-menu.ts"));
const { failureReport, technicalDetail } = await import(await load("src/lib/utils/error-message.ts"));
const settle = async () => {
  for (let index = 0; index < 6; index++) { await new Promise(resolve => setImmediate(resolve)); flushSync(); }
};
const at = id => document.getElementById(id);

test("the native context menu is allowed on fields, editable content and selectable text only", () => {
  for (const id of ["field", "area", "selectable", "editable"]) assert.equal(allowsNativeContextMenu(at(id)), true, id);
  for (const id of ["outside", "plain", "locked"]) assert.equal(allowsNativeContextMenu(at(id)), false, id);
  assert.equal(allowsNativeContextMenu(null), false);
  assert.equal(allowsNativeContextMenu(document), false);
});

test("technical detail is dropped when blank or repeating the summary", () => {
  assert.equal(technicalDetail("Failed.", undefined), null);
  assert.equal(technicalDetail("Failed.", null), null);
  assert.equal(technicalDetail("Failed.", "  \n"), null);
  assert.equal(technicalDetail("Failed.", " Failed. "), null);
  assert.equal(technicalDetail("Failed.", " os error 13 \n"), "os error 13");
});

test("a failure report joins summary and detail for copying", () => {
  assert.equal(failureReport("Failed.", "os error 13"), "Failed.\n\nos error 13");
  assert.equal(failureReport("Failed.", "Failed."), "Failed.");
  assert.equal(failureReport("Failed.", null), "Failed.");
});

test("an error banner keeps the summary visible and the technical detail collapsed and copyable", async () => {
  const written = [];
  Object.defineProperty(globalThis.navigator, "clipboard", { configurable: true, value: { writeText: async text => { written.push(text); } } });
  const ErrorBanner = (await import(await load("src/lib/components/ui/ErrorBanner.svelte"))).default;
  const target = document.createElement("div");
  document.body.append(target);
  const instance = mount(ErrorBanner, { target, props: { message: "Could not save.", detail: "os error 13" } });
  try {
    await settle();
    const alert = target.querySelector('[role="alert"]');
    assert.ok(alert.querySelector("[data-selectable].select-text"));
    const details = alert.querySelector("details");
    assert.equal(details.open, false);
    assert.match(details.textContent, /Dettagli tecnici/);
    assert.equal(details.querySelector("pre").textContent, "os error 13");
    const copy = alert.querySelector('button[aria-label="Copia l\'errore"]');
    copy.click();
    await settle();
    assert.deepEqual(written, ["Could not save.\n\nos error 13"]);
    assert.ok(alert.querySelector('button[aria-label="Copiato"]'));
  } finally { await unmount(instance); target.remove(); }
});

test("an error banner without detail has no details section", async () => {
  const ErrorBanner = (await import(await load("src/lib/components/ui/ErrorBanner.svelte"))).default;
  const target = document.createElement("div");
  const instance = mount(ErrorBanner, { target, props: { message: "Could not save.", detail: "Could not save." } });
  try {
    await settle();
    assert.equal(target.querySelector("details"), null);
  } finally { await unmount(instance); }
});
