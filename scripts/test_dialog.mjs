import assert from "node:assert/strict";
import { test } from "node:test";
import { JSDOM } from "jsdom";
import { compile } from "svelte/compiler";
import { createModuleLoader, dataModule } from "./test_module_loader.mjs";

const dom = new JSDOM("<!doctype html><html><body></body></html>", { url: "http://localhost" });
for (const key of ["window", "document", "navigator", "HTMLElement", "HTMLDialogElement", "HTMLMediaElement", "Element", "Node", "Text", "Comment", "Event", "MouseEvent", "CustomEvent", "MutationObserver"]) {
  Object.defineProperty(globalThis, key, { configurable: true, value: dom.window[key] });
}
globalThis.matchMedia = () => ({ matches: false });
globalThis.requestAnimationFrame = (callback) => setTimeout(callback, 0);
globalThis.cancelAnimationFrame = clearTimeout;
HTMLDialogElement.prototype.showModal = function () { this.setAttribute("open", ""); };
HTMLDialogElement.prototype.close = function () { this.removeAttribute("open"); };

const { mount, unmount, flushSync } = await import("svelte");
const { resolveImports } = createModuleLoader({});
const source = `<script>
  import Dialog from "../src/lib/components/ui/Dialog.svelte";
  let { onClosed } = $props();
  let showing = $state(true);
</script>
{#if showing}
  <Dialog open title="Settings" onClose={() => { showing = false; onClosed(); }}>
    {#snippet actions(close)}<button onclick={close}>Close</button>{/snippet}
    {#snippet children(dismiss)}<button onclick={dismiss}>Cancel</button>{/snippet}
  </Dialog>
{/if}`;
const compiled = compile(source, { generate: "client" }).js.code;
const component = (await import(dataModule(await resolveImports(compiled, "scripts/dialog_test_wrapper.svelte")))).default;

for (const label of ["Close", "Cancel"]) test(`${label} keeps the dialog mounted until its exit finishes`, async () => {
  let closed = 0;
  const instance = mount(component, { target: document.body, props: { onClosed: () => closed++ } });
  await new Promise((resolve) => setTimeout(resolve, 20));
  flushSync();
  const dialog = document.querySelector("dialog");
  assert.equal(dialog.dataset.visible, "true");
  [...dialog.querySelectorAll("button")].find((button) => button.textContent === label).click();
  flushSync();
  assert.equal(dialog.dataset.visible, "false");
  assert.equal(dialog.open, true);
  assert.equal(closed, 0);
  await new Promise((resolve) => setTimeout(resolve, 220));
  flushSync();
  assert.equal(closed, 1);
  assert.equal(document.querySelector("dialog"), null);
  await unmount(instance);
});
