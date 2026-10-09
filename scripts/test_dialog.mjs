import assert from "node:assert/strict";
import { test } from "node:test";
import { readFile } from "node:fs/promises";
import { JSDOM } from "jsdom";
import { compile } from "svelte/compiler";
import { createModuleLoader, dataModule } from "./test_module_loader.mjs";

const dom = new JSDOM("<!doctype html><html><body></body></html>", { url: "http://localhost" });
for (const key of ["window", "document", "navigator", "HTMLElement", "HTMLDialogElement", "HTMLMediaElement", "Element", "Node", "Text", "Comment", "Event", "MouseEvent", "CustomEvent", "MutationObserver"]) {
  Object.defineProperty(globalThis, key, { configurable: true, value: dom.window[key] });
}
globalThis.matchMedia = () => ({ matches: false });
const frames = new Map();
let nextFrame = 0;
globalThis.requestAnimationFrame = callback => { frames.set(++nextFrame, callback); return nextFrame; };
globalThis.cancelAnimationFrame = id => frames.delete(id);
const advanceFrame = () => {
  const scheduled = [...frames];
  frames.clear();
  for (const [, callback] of scheduled) callback(0);
};
HTMLDialogElement.prototype.showModal = function () { this.setAttribute("open", ""); };
HTMLDialogElement.prototype.close = function () { this.removeAttribute("open"); };

const appearanceCss = await readFile(new URL("../src/app.css", import.meta.url), "utf8");
const style = document.createElement("style");
// jsdom incorrectly applies grouped pseudo-element selectors to ordinary elements.
style.textContent = appearanceCss.slice(appearanceCss.indexOf("html, body, #app"), appearanceCss.indexOf('input[type="radio"]'))
  .replace(/([^{}]+)\{([^{}]*)\}/g, (_rule, selectors, declarations) => {
    const ordinary = selectors.split(",").filter(selector => !selector.includes("::"));
    return ordinary.length ? ordinary.join(",") + "{" + declarations + "}" : "";
  });
document.head.append(style);
const { mount, unmount, flushSync } = await import("svelte");
const { resolveImports } = createModuleLoader({});
const source = `<script>
  import Dialog from "../src/lib/components/ui/Dialog.svelte";
  let { onClosed, persistent = false } = $props();
  let showing = $state(true);
</script>
{#if showing || persistent}
  <Dialog open={showing} title="Settings" onClose={() => { showing = false; onClosed(); }}>
    {#snippet actions(close)}<button onclick={close}>Close</button>{/snippet}
    {#snippet children(dismiss)}<button onclick={dismiss}>Cancel</button>{/snippet}
  </Dialog>
{/if}
<button onclick={() => showing = true}>Reopen</button>
<button onclick={() => showing = false}>External close</button>`;
const compiled = compile(source, { generate: "client" }).js.code;
const component = (await import(dataModule(await resolveImports(compiled, "scripts/dialog_test_wrapper.svelte")))).default;

for (const label of ["Close", "Cancel"]) test(`${label} keeps the dialog mounted until its exit finishes`, async () => {
  let closed = 0;
  const instance = mount(component, { target: document.body, props: { onClosed: () => closed++ } });
  flushSync();
  advanceFrame();
  advanceFrame();
  flushSync();
  const dialog = document.querySelector("dialog");
  assert.equal(dialog.dataset.visible, "true");
  [...dialog.querySelectorAll("button")].find((button) => button.textContent === label).click();
  flushSync();
  assert.equal(dialog.dataset.visible, "false");
  assert.equal(dialog.open, true);
  assert.equal(closed, 0);
  await new Promise((resolve) => setTimeout(resolve, 260));
  flushSync();
  assert.equal(closed, 1);
  assert.equal(document.querySelector("dialog"), null);
  await unmount(instance);
});


test("dialog fades its surface and content without isolating the surface backdrop", async () => {
  const instance = mount(component, { target: document.body, props: { onClosed: () => {} } });
  flushSync();
  const dialog = document.querySelector("dialog");
  try {
    assert.equal(dom.window.getComputedStyle(dialog).opacity, "1", "opening dialog must not isolate the blurred surface");
    const content = dialog.firstElementChild;
    assert.equal(dom.window.getComputedStyle(content).opacity, "0");
    advanceFrame();
    advanceFrame();
    flushSync();
    assert.equal(dom.window.getComputedStyle(dialog).opacity, "1");
    assert.equal(dom.window.getComputedStyle(content).opacity, "1");
    [...dialog.querySelectorAll("button")].find(button => button.textContent === "Close").click();
    flushSync();
    assert.equal(dom.window.getComputedStyle(dialog).opacity, "1", "closing dialog must retain access to the page backdrop");
    assert.equal(dom.window.getComputedStyle(content).opacity, "0");
  } finally {
    await unmount(instance);
  }
});


function finishOpacityTransition(content) {
  const event = new Event("transitionend", { bubbles: true });
  Object.defineProperty(event, "propertyName", { value: "opacity" });
  content.dispatchEvent(event);
  flushSync();
}

test("closing waits for the content fade and hides the dialog before leaving the top layer", async () => {
  let closed = 0;
  const instance = mount(component, { target: document.body, props: { onClosed: () => closed++, persistent: true } });
  try {
    flushSync();
    advanceFrame();
    advanceFrame();
    const dialog = document.querySelector("dialog");
    const content = dialog.firstElementChild;
    [...dialog.querySelectorAll("button")].find(button => button.textContent === "Close").click();
    flushSync();
    finishOpacityTransition(content.querySelector("button"));
    assert.equal(dialog.open, true, "nested transitions must not finish the dialog exit");
    assert.equal(closed, 0);
    finishOpacityTransition(content);
    assert.equal(dialog.open, false);
    assert.equal(dialog.style.visibility, "hidden", "the closed dialog must remain hidden if the native top layer paints another frame");
    assert.equal(closed, 1);
    advanceFrame();
    advanceFrame();
    flushSync();
    assert.equal(dialog.open, false, "an opening frame must not bring the closed dialog back");
    assert.equal(dialog.dataset.visible, "false");
  } finally { await unmount(instance); }
});

test("reopening during an external close reverses the fade and cancels the pending close", async () => {
  let closed = 0;
  const instance = mount(component, { target: document.body, props: { onClosed: () => closed++, persistent: true } });
  try {
    flushSync();
    advanceFrame();
    advanceFrame();
    const dialog = document.querySelector("dialog");
    [...document.querySelectorAll("button")].find(button => button.textContent === "External close").click();
    flushSync();
    assert.equal(dialog.dataset.visible, "false");
    [...document.querySelectorAll("button")].find(button => button.textContent === "Reopen").click();
    flushSync();
    assert.equal(dialog.dataset.visible, "true");
    finishOpacityTransition(dialog.firstElementChild);
    await new Promise(resolve => setTimeout(resolve, 260));
    assert.equal(dialog.open, true);
    assert.equal(closed, 0);
  } finally { await unmount(instance); }
});
