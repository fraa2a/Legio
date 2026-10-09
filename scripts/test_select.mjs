import assert from "node:assert/strict";
import { test } from "node:test";
import { JSDOM } from "jsdom";
import { createModuleLoader } from "./test_module_loader.mjs";

const dom = new JSDOM("<!doctype html><html><body></body></html>", { pretendToBeVisual: true });
for (const key of ["window", "document", "navigator", "Node", "Text", "Comment", "Element", "HTMLElement", "HTMLMediaElement", "Event", "KeyboardEvent", "MouseEvent"]) {
  Object.defineProperty(globalThis, key, { configurable: true, value: dom.window[key] });
}
const popovers = new Set();
HTMLElement.prototype.showPopover = function () { popovers.add(this); };
const { mount, unmount, flushSync } = await import("svelte");
const { load } = createModuleLoader({});
const SelectField = (await import(await load("src/lib/components/ui/SelectField.svelte"))).default;
const settle = async () => {
  for (let i = 0; i < 4; i++) { await new Promise(resolve => setImmediate(resolve)); flushSync(); }
};

async function fixture(top) {
  const target = document.createElement("div");
  document.body.append(target);
  const chosen = [];
  const instance = mount(SelectField, { target, props: {
    id: "language", label: "Language", value: "en",
    options: [{ value: "en", label: "English" }, { value: "it", label: "Italiano" }],
    onChange: value => chosen.push(value),
  } });
  await settle();
  const trigger = target.querySelector("button");
  trigger.getBoundingClientRect = () => ({ top, bottom: top + 40, left: 130, width: 240 });
  Object.defineProperty(HTMLElement.prototype, "scrollHeight", { configurable: true, get() { return 96; } });
  return { target, trigger, chosen, instance };
}

test("dropdown opens above dialog content with viewport coordinates and keyboard selection", async () => {
  const { target, trigger, chosen, instance } = await fixture(120);
  try {
    trigger.click(); await settle();
    const list = target.querySelector('[role="listbox"]');
    assert.ok(popovers.has(list), "the native top layer must escape filtered and clipped ancestors");
    assert.equal(list.style.top, "168px");
    assert.equal(list.style.left, "130px");
    assert.equal(list.style.width, "240px");
    assert.equal(document.activeElement.textContent.trim(), "English");
    document.activeElement.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowDown", bubbles: true }));
    assert.equal(document.activeElement.textContent.trim(), "Italiano");
    document.activeElement.click(); await settle();
    assert.deepEqual(chosen, ["it"]);
    assert.equal(target.querySelector('[role="listbox"]'), null);
    assert.equal(document.activeElement, trigger);
  } finally { await unmount(instance); target.remove(); popovers.clear(); }
});

test("dropdown fits above a low trigger and native dismissal resets expanded state", async () => {
  const { target, trigger, instance } = await fixture(700);
  try {
    trigger.click(); await settle();
    const list = target.querySelector('[role="listbox"]');
    assert.equal(list.style.top, "596px");
    const dismissal = new Event("toggle");
    Object.defineProperty(dismissal, "newState", { value: "closed" });
    list.dispatchEvent(dismissal); await settle();
    assert.equal(trigger.getAttribute("aria-expanded"), "false");
    assert.equal(target.querySelector('[role="listbox"]'), null);
  } finally { await unmount(instance); target.remove(); popovers.clear(); }
});
