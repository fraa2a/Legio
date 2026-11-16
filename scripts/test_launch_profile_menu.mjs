import assert from "node:assert/strict";
import { test } from "node:test";
import { readFile } from "node:fs/promises";
import { JSDOM } from "jsdom";
import { createModuleLoader, dataModule } from "./test_module_loader.mjs";

const dom = new JSDOM("<!doctype html><html><body></body></html>", { pretendToBeVisual: true });
for (const key of ["window", "document", "navigator", "Node", "Text", "Comment", "Element", "HTMLElement", "HTMLMediaElement", "Event", "KeyboardEvent", "MouseEvent", "CustomEvent", "getComputedStyle"]) {
  Object.defineProperty(globalThis, key, { configurable: true, value: dom.window[key] });
}
const { mount, unmount, flushSync } = await import("svelte");
const english = JSON.parse(await readFile(new URL("../src/lib/i18n/en.json", import.meta.url), "utf8"));
const { load } = createModuleLoader({
  "src/lib/utils/motion": dataModule("export const reducedMotion = true;"),
  "src/lib/i18n": dataModule(`import { writable } from ${JSON.stringify(import.meta.resolve("svelte/store"))}; export const language = writable("en"); const english = ${JSON.stringify(english)}; export const t = (value, _language, values = []) => (english[value] ?? value).replace(/\\{(\\d+)\\}/g, (_, index) => String(values[Number(index)] ?? ""));`),
});
const Split = (await import(await load("src/lib/features/library/LaunchProfileSplit.svelte"))).default;

const settle = async () => { for (let i = 0; i < 4; i++) { await new Promise(setImmediate); flushSync(); } };
const options = [{ id: null, name: "Default" }, { id: "a", name: "Vanilla" }, { id: "b", name: "Modded" }];

async function withSplit(props, run) {
  const target = document.createElement("div");
  document.body.append(target);
  const launched = [];
  const instance = mount(Split, { target, props: { label: "PLAY", options, selectedId: "a", onPlay: (id) => launched.push(id), ...props } });
  try {
    await settle();
    await run(target, launched);
  } finally { await unmount(instance); target.remove(); }
}

test("the main button launches the selected profile without opening the menu", async () => {
  await withSplit({}, async (target, launched) => {
    const [play, trigger] = target.querySelectorAll("button");
    assert.match(play.textContent, /PLAY/);
    assert.match(play.title, /Vanilla/);
    assert.equal(trigger.getAttribute("aria-expanded"), "false");
    play.click(); await settle();
    assert.deepEqual(launched, ["a"]);
    assert.equal(target.querySelector('[role="menu"]'), null);
  });
});

test("the menu lists the base settings first, marks the selection and launches the choice", async () => {
  await withSplit({}, async (target, launched) => {
    const trigger = target.querySelectorAll("button")[1];
    trigger.click(); await settle();
    assert.equal(trigger.getAttribute("aria-expanded"), "true");
    const items = [...target.querySelectorAll('[role="menuitemradio"]')];
    assert.deepEqual(items.map((item) => item.textContent.trim()), ["Default", "Vanilla", "Modded"]);
    assert.deepEqual(items.map((item) => item.getAttribute("aria-checked")), ["false", "true", "false"]);
    assert.equal(document.activeElement, items[1], "the selected entry takes focus");
    items[2].click(); await settle();
    assert.deepEqual(launched, ["b"]);
    assert.equal(target.querySelector('[role="menu"]'), null);
  });
});

test("choosing the base settings launches a null profile", async () => {
  await withSplit({ selectedId: null }, async (target, launched) => {
    target.querySelectorAll("button")[1].click(); await settle();
    target.querySelector('[role="menuitemradio"]').click(); await settle();
    assert.deepEqual(launched, [null]);
  });
});

test("arrow keys move through the entries and Escape closes the menu to the trigger", async () => {
  await withSplit({}, async (target) => {
    const trigger = target.querySelectorAll("button")[1];
    trigger.focus();
    trigger.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowDown", bubbles: true })); await settle();
    const items = [...target.querySelectorAll('[role="menuitemradio"]')];
    assert.equal(document.activeElement, items[1]);
    document.activeElement.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowDown", bubbles: true }));
    assert.equal(document.activeElement, items[2]);
    document.activeElement.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowDown", bubbles: true }));
    assert.equal(document.activeElement, items[0], "navigation wraps");
    document.activeElement.dispatchEvent(new KeyboardEvent("keydown", { key: "End", bubbles: true }));
    assert.equal(document.activeElement, items[2]);
    document.activeElement.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true })); await settle();
    assert.equal(target.querySelector('[role="menu"]'), null);
    assert.equal(document.activeElement, trigger);
  });
});

test("a disabled control cannot launch or open", async () => {
  await withSplit({ disabled: true }, async (target, launched) => {
    const [play, trigger] = target.querySelectorAll("button");
    assert.equal(play.disabled, true);
    assert.equal(trigger.disabled, true);
    play.click(); trigger.click(); await settle();
    assert.deepEqual(launched, []);
    assert.equal(target.querySelector('[role="menu"]'), null);
  });
});
