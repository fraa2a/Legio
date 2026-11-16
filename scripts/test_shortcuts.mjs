import assert from "node:assert/strict";
import { test } from "node:test";
import { JSDOM } from "jsdom";

const dom = new JSDOM("<!doctype html><html><body></body></html>");
for (const key of ["Element", "HTMLElement", "HTMLInputElement", "HTMLTextAreaElement", "HTMLSelectElement"]) {
  Object.defineProperty(globalThis, key, { configurable: true, value: dom.window[key] });
}
const { matchShortcut, shortcutScope, sectionShortcutCount } = await import("../src/lib/features/shortcuts/shortcut-model.ts");

const free = { editing: false, control: false, dialog: false };
const key = (name, extra = {}) => ({ key: name, ctrlKey: false, metaKey: false, altKey: false, ...extra });
const ctrl = (name, extra = {}) => key(name, { ctrlKey: true, ...extra });

test("Ctrl+1 to Ctrl+4 select the main views by position", () => {
  for (let index = 0; index < sectionShortcutCount; index++) {
    assert.deepEqual(matchShortcut(ctrl(String(index + 1)), free), { type: "section", index });
  }
  assert.equal(matchShortcut(ctrl("5"), free), null);
  assert.equal(matchShortcut(ctrl("0"), free), null);
  assert.equal(matchShortcut(key("1"), free), null);
});

test("Ctrl+K works everywhere and Meta counts as Ctrl", () => {
  const everywhere = { editing: true, control: true, dialog: true };
  assert.deepEqual(matchShortcut(ctrl("k"), everywhere), { type: "palette" });
  assert.deepEqual(matchShortcut(key("K", { metaKey: true }), free), { type: "palette" });
  assert.equal(matchShortcut(ctrl("k", { altKey: true }), free), null);
});

test("Ctrl+F focuses the search, also from a field", () => {
  assert.deepEqual(matchShortcut(ctrl("f"), free), { type: "search" });
  assert.deepEqual(matchShortcut(ctrl("F"), { ...free, editing: true }), { type: "search" });
});

test("open dialogs only keep Ctrl+K", () => {
  const dialog = { ...free, dialog: true };
  for (const event of [ctrl("1"), ctrl("f"), key("?"), key("Enter")]) assert.equal(matchShortcut(event, dialog), null);
});

test("bare keys stay out of fields and controls", () => {
  assert.deepEqual(matchShortcut(key("Enter"), free), { type: "play" });
  assert.deepEqual(matchShortcut(key("?", { shiftKey: true }), free), { type: "help" });
  assert.equal(matchShortcut(key("Enter"), { ...free, editing: true }), null);
  assert.equal(matchShortcut(key("?"), { ...free, editing: true }), null);
  assert.equal(matchShortcut(key("Enter"), { ...free, control: true }), null);
  assert.deepEqual(matchShortcut(key("?"), { ...free, control: true }), { type: "help" });
  assert.equal(matchShortcut(key("Enter", { altKey: true }), free), null);
});

test("focus scope tells fields, controls and dialogs apart", () => {
  const html = (markup) => { dom.window.document.body.innerHTML = markup; return dom.window.document.body.firstElementChild; };
  assert.deepEqual(shortcutScope(html("<input type='search'>"), false), { editing: true, control: false, dialog: false });
  assert.equal(shortcutScope(html("<textarea></textarea>"), false).editing, true);
  assert.equal(shortcutScope(html("<select></select>"), false).editing, true);
  const editable = html("<div contenteditable='true'></div>");
  Object.defineProperty(editable, "isContentEditable", { value: true });
  assert.equal(shortcutScope(editable, false).editing, true);
  assert.deepEqual(shortcutScope(html("<button><span>x</span></button>").firstElementChild, false), { editing: false, control: true, dialog: false });
  assert.equal(shortcutScope(html("<input type='checkbox'>"), false).control, true);
  assert.equal(shortcutScope(html("<a href='#'>x</a>"), false).control, true);
  assert.deepEqual(shortcutScope(html("<div></div>"), true), { editing: false, control: false, dialog: true });
  assert.deepEqual(shortcutScope(null, false), { editing: false, control: false, dialog: false });
});
