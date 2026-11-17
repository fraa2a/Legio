import assert from "node:assert/strict";
import { test } from "node:test";
import { readdir, readFile } from "node:fs/promises";
import { JSDOM } from "jsdom";
import { compile } from "svelte/compiler";
import { createModuleLoader, dataModule } from "./test_module_loader.mjs";

const dom = new JSDOM("<!doctype html><html><body></body></html>", { url: "http://localhost" });
for (const key of ["window", "document", "navigator", "HTMLElement", "HTMLDialogElement", "HTMLMediaElement", "Element", "Node", "Text", "Comment", "Event", "MouseEvent", "KeyboardEvent", "CustomEvent", "MutationObserver"]) {
  Object.defineProperty(globalThis, key, { configurable: true, value: dom.window[key] });
}
globalThis.matchMedia = () => ({ matches: false });
globalThis.requestAnimationFrame = (callback) => setTimeout(() => callback(0), 0);
globalThis.cancelAnimationFrame = (id) => clearTimeout(id);

// jsdom has no top layer: this mirrors the HTML spec, where close() returns focus to the element
// that was focused when showModal() ran.
const modalOwners = new WeakMap();
HTMLDialogElement.prototype.showModal = function () {
  modalOwners.set(this, document.activeElement);
  this.setAttribute("open", "");
  this.modal = true;
};
HTMLDialogElement.prototype.close = function () {
  this.removeAttribute("open");
  this.modal = false;
  modalOwners.get(this)?.focus();
};

const { mount, unmount, flushSync } = await import("svelte");
const mocks = {
  "src/lib/i18n": dataModule('import { writable } from "' + import.meta.resolve("svelte/store") + '"; export const language = writable("it"); export const t = (value, _language, args = []) => value.replace(/\\{(\\d+)\\}/g, (_, index) => args[index]);'),
  "src/lib/utils/motion": dataModule("export const reducedMotion = true; export const fadeDuration = 0;"),
};
const { resolveImports } = createModuleLoader(mocks);

const source = `<script>
  import Button from "../src/lib/components/ui/Button.svelte";
  import CloseButton from "../src/lib/components/ui/CloseButton.svelte";
  import ConfirmDialog from "../src/lib/components/ui/ConfirmDialog.svelte";
  import Dialog from "../src/lib/components/ui/Dialog.svelte";
  import Icon from "../src/lib/components/ui/Icon.svelte";
  import IconButton from "../src/lib/components/ui/IconButton.svelte";
  import Pagination from "../src/lib/components/ui/Pagination.svelte";
  import RadioCard from "../src/lib/components/ui/RadioCard.svelte";
  import SegmentedControl from "../src/lib/components/ui/SegmentedControl.svelte";
  import SelectField from "../src/lib/components/ui/SelectField.svelte";
  import Slider from "../src/lib/components/ui/Slider.svelte";
  import TextField from "../src/lib/components/ui/TextField.svelte";
  import Toggle from "../src/lib/components/ui/Toggle.svelte";
  let { events } = $props();
  let open = $state(false);
  let confirming = $state(false);
  let mode = $state("a");
  let size = $state(5);
  let text = $state("");
</script>
<main>
  <button id="opener" onclick={() => open = true}>Open</button>
  <button id="confirm-opener" onclick={() => confirming = true}>Confirm</button>
  <div id="icons">
    <IconButton label="Refresh" onClick={() => {}}><Icon name="close" /></IconButton>
    <IconButton label="Remove" title="Remove tooltip" onClick={() => {}}><Icon name="close" /></IconButton>
    <CloseButton label="Dismiss" onClick={() => {}} />
    <Button label="Round" shape="circle" onClick={() => {}}><Icon name="close" /></Button>
  </div>
  <div id="pages"><Pagination page={2} pageCount={4} onChange={() => {}} /></div>
  <div id="toggle"><Toggle label="Notifications" checked={false} /></div>
  <div id="slider"><Slider label="Volume" value={size} min={0} max={10} valueText="5 of 10" onInput={(value) => { size = value; events.push(value); }} /></div>
  <div id="segments"><SegmentedControl name="mode" label="Mode" options={[{ value: "a", label: "A" }, { value: "b", label: "B" }]} value={mode} onChange={(value) => mode = value} /></div>
  <div id="cards">
    <RadioCard name="card" value="x" checked={true} variant="tile" onSelect={() => {}}>One</RadioCard>
    <RadioCard name="card" value="y" checked={false} variant="tile" onSelect={() => {}}>Two</RadioCard>
  </div>
  <div id="select"><SelectField id="lang" label="Language" value="en" options={[{ value: "en", label: "English" }]} /></div>
  <div id="text"><TextField id="name" label="Name" bind:value={text} hint="A hint" /><TextField id="search" type="search" label="Search" labelHidden bind:value={text} /></div>
</main>
<Dialog {open} title="Settings" onClose={() => { open = false; events.push("closed"); }}>
  {#snippet actions(close)}<CloseButton label="Close settings" onClick={close} />{/snippet}
  {#snippet children()}<button id="inside">Inside</button>{/snippet}
</Dialog>
<ConfirmDialog open={confirming} title="Delete" message="Sure?" confirmLabel="Delete" tone="danger" onConfirm={() => {}} onClose={() => confirming = false} />`;
const compiled = compile(source, { generate: "client" }).js.code;
const Page = (await import(dataModule(await resolveImports(compiled, "scripts/accessibility_wrapper.svelte")))).default;

const settle = async () => {
  for (let index = 0; index < 4; index++) { await new Promise((resolve) => setTimeout(resolve, 5)); flushSync(); }
};

async function mountPage() {
  const events = [];
  const instance = mount(Page, { target: document.body, props: { events } });
  await settle();
  return { events, cleanup: () => { unmount(instance); document.body.replaceChildren(); } };
}

const accessibleName = (element) => {
  const label = element.getAttribute("aria-label");
  if (label) return label.trim();
  const labelledBy = element.getAttribute("aria-labelledby");
  if (labelledBy) return labelledBy.split(" ").map((id) => document.getElementById(id)?.textContent ?? "").join(" ").trim();
  if (element.labels?.length) return [...element.labels].map((item) => item.textContent).join(" ").trim();
  return element.textContent.trim();
};

test("every button and form control in the primitives has an accessible name", async () => {
  const { cleanup } = await mountPage();
  try {
    const controls = [...document.querySelectorAll("button, input, select, textarea")];
    assert.ok(controls.length >= 20);
    for (const control of controls) assert.notEqual(accessibleName(control), "", control.outerHTML.slice(0, 120));
    for (const icon of document.querySelectorAll("#icons svg")) assert.equal(icon.getAttribute("aria-hidden"), "true");
    assert.equal(document.querySelector("#icons button[aria-label='Remove']").getAttribute("aria-label"), "Remove");
  } finally { cleanup(); }
});

test("pagination is a labelled nav and marks the current page", async () => {
  const { cleanup } = await mountPage();
  try {
    const nav = document.querySelector("#pages nav");
    assert.equal(nav.getAttribute("aria-label"), "Pagine");
    const current = [...nav.querySelectorAll("button")].filter((button) => button.getAttribute("aria-pressed") === "true");
    assert.deepEqual(current.map((button) => button.getAttribute("aria-label")), ["Pagina 2"]);
  } finally { cleanup(); }
});

test("toggles expose a switch and sliders a named native range", async () => {
  const { events, cleanup } = await mountPage();
  try {
    const toggle = document.querySelector("#toggle input");
    assert.equal(toggle.type, "checkbox");
    assert.equal(toggle.getAttribute("role"), "switch");
    const slider = document.querySelector("#slider input");
    assert.equal(slider.type, "range");
    assert.equal(slider.getAttribute("aria-label"), "Volume");
    assert.equal(slider.getAttribute("aria-valuetext"), "5 of 10");
    assert.equal(slider.getAttribute("tabindex"), null, "a native range is already focusable");
    slider.value = "6";
    slider.dispatchEvent(new window.Event("input", { bubbles: true }));
    assert.deepEqual(events, [6]);
  } finally { cleanup(); }
});

test("segments and radio cards are native radio groups, so the browser provides arrow keys", async () => {
  const { cleanup } = await mountPage();
  try {
    const group = document.querySelector("#segments fieldset");
    assert.equal(group.querySelector("legend").textContent, "Mode");
    const radios = [...group.querySelectorAll("input")];
    assert.deepEqual(radios.map((radio) => [radio.type, radio.name, radio.checked]), [["radio", "mode", true], ["radio", "mode", false]]);
    const cards = [...document.querySelectorAll("#cards input")];
    assert.equal(new Set(cards.map((card) => card.name)).size, 1);
    for (const input of [...radios, ...cards]) assert.equal(input.hasAttribute("tabindex"), false, "no tabindex that would break the native roving focus");
    for (const input of document.querySelectorAll("#segments input, #cards input")) assert.ok(input.labels.length > 0);
  } finally { cleanup(); }
});

test("select triggers and text fields announce their state and help text", async () => {
  const { cleanup } = await mountPage();
  try {
    const trigger = document.querySelector("#select button");
    assert.equal(trigger.getAttribute("aria-haspopup"), "listbox");
    assert.equal(trigger.getAttribute("aria-expanded"), "false");
    assert.equal(accessibleName(trigger), "Language");
    const field = document.querySelector("#name");
    assert.equal(document.getElementById(field.getAttribute("aria-describedby")).textContent, "A hint");
  } finally { cleanup(); }
});

test("a dialog is modal, labelled, closes on Escape and returns focus to its opener", async () => {
  const { events, cleanup } = await mountPage();
  try {
    const opener = document.querySelector("#opener");
    opener.focus();
    opener.click();
    await settle();
    const dialog = [...document.querySelectorAll("dialog")].find((item) => item.hasAttribute("open"));
    assert.ok(dialog.modal, "showModal makes the rest of the page inert and traps Tab");
    assert.equal(document.getElementById(dialog.getAttribute("aria-labelledby")).textContent, "Settings");
    assert.equal(document.getElementById(dialog.getAttribute("aria-labelledby")).tagName, "H2");
    assert.equal(document.activeElement, dialog, "focus moves into the dialog on open");
    assert.ok(dialog.querySelector("button[aria-label='Close settings']"));
    dialog.dispatchEvent(new window.KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true }));
    await settle();
    assert.deepEqual(events, ["closed"]);
    assert.equal(dialog.hasAttribute("open"), false);
    assert.equal(document.activeElement, opener, "focus returns to the element that opened the dialog");
  } finally { cleanup(); }
});

test("a confirm dialog names both actions", async () => {
  const { cleanup } = await mountPage();
  try {
    document.querySelector("#confirm-opener").click();
    await settle();
    const dialog = [...document.querySelectorAll("dialog")].find((item) => item.hasAttribute("open"));
    assert.deepEqual([...dialog.querySelectorAll("button")].map(accessibleName), ["Annulla", "Delete"]);
  } finally { cleanup(); }
});

test("sources have no positive tabindex and every image carries an alt attribute", async () => {
  const files = [];
  const walk = async (directory) => {
    for (const entry of await readdir(directory, { withFileTypes: true })) {
      const path = new URL(entry.name + (entry.isDirectory() ? "/" : ""), directory);
      if (entry.isDirectory()) await walk(path);
      else if (entry.name.endsWith(".svelte")) files.push(path);
    }
  };
  await walk(new URL("../src/", import.meta.url));
  assert.ok(files.length > 50);
  for (const file of files) {
    const markup = (await readFile(file, "utf8")).replace(/<script[\s\S]*?<\/script>/g, "");
    assert.doesNotMatch(markup, /tabindex=["{]\s*[1-9]/, file.pathname + " uses a positive tabindex");
    for (const image of markup.match(/<img\b[^>]*>/g) ?? []) assert.match(image, /\balt[=\s{}]|\{alt\}/, file.pathname + " has an image without alt: " + image.slice(0, 80));
  }
});
