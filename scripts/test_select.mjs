import assert from "node:assert/strict";
import { test } from "node:test";
import { JSDOM } from "jsdom";
import { createModuleLoader, dataModule } from "./test_module_loader.mjs";

const dom = new JSDOM("<!doctype html><html><body></body></html>", { pretendToBeVisual: true });
for (const key of ["window", "document", "navigator", "Node", "Text", "Comment", "Element", "HTMLElement", "HTMLMediaElement", "Event", "KeyboardEvent", "MouseEvent"]) {
  Object.defineProperty(globalThis, key, { configurable: true, value: dom.window[key] });
}
const popovers = new Set();
HTMLElement.prototype.showPopover = function () { popovers.add(this); };
const animations = [];
HTMLElement.prototype.animate = function (keyframes, options) {
  const animation = { keyframes, options, onfinish: null };
  animations.push(animation);
  return animation;
};
const { mount, unmount, flushSync } = await import("svelte");
const component = async (reduced) => {
  const { load } = createModuleLoader({ "src/lib/utils/motion": dataModule(`export const reducedMotion = ${reduced};`) });
  return (await import(await load("src/lib/components/ui/SelectField.svelte"))).default;
};
const ReducedSelect = await component(true);
const AnimatedSelect = await component(false);
const settle = async () => {
  for (let i = 0; i < 4; i++) { await new Promise(resolve => setImmediate(resolve)); flushSync(); }
};
const key = (name) => document.activeElement.dispatchEvent(new KeyboardEvent("keydown", { key: name, bubbles: true }));

async function fixture(top, Component = ReducedSelect) {
  const target = document.createElement("div");
  document.body.append(target);
  const chosen = [];
  const instance = mount(Component, { target, props: {
    id: "language", label: "Language", value: "en",
    options: [{ value: "en", label: "English" }, { value: "it", label: "Italiano" }, { value: "de", label: "Deutsch" }],
    onChange: value => chosen.push(value),
  } });
  await settle();
  const trigger = target.querySelector("button");
  trigger.getBoundingClientRect = () => ({ top, bottom: top + 40, left: 130, width: 240 });
  Object.defineProperty(HTMLElement.prototype, "scrollHeight", { configurable: true, get() { return 96; } });
  const cleanup = async () => { await unmount(instance); target.remove(); popovers.clear(); animations.length = 0; };
  return { target, trigger, chosen, cleanup };
}

test("the list grows out of the trigger edge as one surface and supports the keyboard", async () => {
  const { target, trigger, chosen, cleanup } = await fixture(120);
  try {
    trigger.click(); await settle();
    const list = target.querySelector('[role="listbox"]');
    assert.ok(popovers.has(list), "the native top layer must escape filtered and clipped ancestors");
    assert.equal(trigger.getAttribute("aria-expanded"), "true");
    assert.equal(list.style.top, "160px", "the list touches the trigger bottom edge");
    assert.equal(list.style.left, "130px");
    assert.equal(list.style.width, "240px");
    assert.ok(trigger.classList.contains("rounded-t-lg") && list.classList.contains("rounded-b-lg"));
    assert.ok(trigger.classList.contains("legio-popover") && list.classList.contains("legio-popover"), "trigger and list share the surface");
    assert.ok(!list.classList.contains("border-t"), "the trigger border is the only seam");
    assert.equal(document.activeElement.textContent.trim(), "English");
    key("End");
    assert.equal(document.activeElement.textContent.trim(), "Deutsch");
    key("Home"); key("ArrowDown");
    assert.equal(document.activeElement.textContent.trim(), "Italiano");
    document.activeElement.click(); await settle();
    assert.deepEqual(chosen, ["it"]);
    assert.equal(target.querySelector('[role="listbox"]'), null);
    assert.equal(document.activeElement, trigger);
    assert.ok(trigger.classList.contains("rounded-lg"), "corners return once closed");
    assert.equal(animations.length, 0, "reduced motion skips the animation");
  } finally { await cleanup(); }
});

test("a low trigger grows the list upwards and Escape closes it", async () => {
  const { target, trigger, cleanup } = await fixture(700);
  try {
    trigger.focus();
    key("ArrowDown"); await settle();
    const list = target.querySelector('[role="listbox"]');
    assert.equal(list.style.bottom, `${window.innerHeight - 700}px`);
    assert.equal(list.style.top, "auto");
    assert.ok(trigger.classList.contains("rounded-b-lg") && list.classList.contains("rounded-t-lg"));
    key("Escape"); await settle();
    assert.equal(trigger.getAttribute("aria-expanded"), "false");
    assert.equal(target.querySelector('[role="listbox"]'), null);
    assert.equal(document.activeElement, trigger);
  } finally { await cleanup(); }
});

test("opening and closing both animate before the list is removed", async () => {
  const { target, trigger, cleanup } = await fixture(120, AnimatedSelect);
  try {
    trigger.click(); await settle();
    assert.equal(animations.length, 1);
    assert.equal(animations[0].keyframes[0].height, "0px");
    trigger.click(); await settle();
    assert.equal(animations.length, 2);
    assert.equal(animations[1].keyframes.at(-1).height, "0px");
    assert.equal(trigger.getAttribute("aria-expanded"), "false");
    assert.ok(target.querySelector('[role="listbox"]'), "the list stays while it collapses");
    animations[1].onfinish(); await settle();
    assert.equal(target.querySelector('[role="listbox"]'), null);
  } finally { await cleanup(); }
});

test("clicking outside closes the list", async () => {
  const { target, trigger, cleanup } = await fixture(120);
  try {
    trigger.click(); await settle();
    document.body.click(); await settle();
    assert.equal(target.querySelector('[role="listbox"]'), null);
  } finally { await cleanup(); }
});
