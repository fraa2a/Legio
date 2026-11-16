import assert from "node:assert/strict";
import { test } from "node:test";
import { readFileSync, readdirSync } from "node:fs";

// Enforces the scales in _meta/docs/UI.md on the Svelte sources.
const frontend = new URL("../src/", import.meta.url);
const files = readdirSync(frontend, { recursive: true })
  .filter(path => path.endsWith(".svelte"))
  .map(path => ({ path: `src/${path}`, primitive: path.includes("components/ui/"), text: withoutComments(readFileSync(new URL(path, frontend), "utf8")) }));

const radii = new Set(["control", "card", "panel", "full"]);
const hues = "red|orange|amber|yellow|lime|green|emerald|teal|cyan|sky|blue|indigo|violet|purple|fuchsia|pink|rose|slate|gray|neutral|stone|taupe|mauve|mist|olive";
const colorUtility = "bg|text|border(?:-[trblxy])?|ring|inset-ring|outline|fill|stroke|from|via|to|shadow|accent|caret|decoration|divide|placeholder";
const paletteColor = new RegExp(`^(?:${colorUtility})-(?:${hues})-\\d{2,3}(?:/\\d+)?$`);
// Off-scale heights that hand-written toolbars and fields used before the control scale.
const offScaleHeight = /^(?:min-)?h-(?:9|11)$/;
// Raw controls outside the primitives take control heights from the tokens. Below 24 px they are
// inline or decorative, the 28 px inline size is allowed, and from 64 px they are tiles.
const numericHeight = /^(?:(?:min-)?h|size)-(\d+(?:\.\d+)?)$/;
const isControlHeight = value => value >= 6 && value <= 15 && value !== 7;

function withoutComments(text) {
  return text
    .replace(/<!--[\s\S]*?-->/g, "")
    .replace(/<style[\s\S]*?<\/style>/g, "")
    .replace(/\/\*[\s\S]*?\*\//g, "")
    .replace(/^\s*\/\/.*$/gm, "");
}

// Class-like tokens with their variants removed, such as light:hover:bg-white -> bg-white.
function tokens(text) {
  return text.split(/[\s"'`{}]+/).map(token => token.split(":").pop().replace(/^!/, "")).filter(Boolean);
}

function violations(rule) {
  return files.flatMap(file => [...new Set(rule(file))].map(token => `${file.path}: ${token}`));
}

// The opening tags of raw controls, skipping ">" inside Svelte expressions.
function controlTags(text) {
  const tags = [];
  for (const match of text.matchAll(/<(?:button|input|select|textarea)\b/g)) {
    let depth = 0;
    let end = match.index;
    for (; end < text.length; end++) {
      if (text[end] === "{") depth++;
      else if (text[end] === "}") depth--;
      else if (text[end] === ">" && depth === 0) break;
    }
    tags.push(text.slice(match.index, end));
  }
  return tags;
}

test("corners use the radius roles", () => {
  assert.deepEqual(violations(file => tokens(file.text).filter(token => {
    const radius = /^rounded(?:-(?:[trblse]|[trbl][lr]|s[se]|e[se]))?(?:-(.+))?$/.exec(token);
    return radius !== null && !radii.has(radius[1]);
  })), []);
});

test("colors come from zinc, white, black and the legio tokens", () => {
  assert.deepEqual(violations(file => tokens(file.text).filter(token => paletteColor.test(token))), []);
});

test("controls outside the primitives use the control heights", () => {
  assert.deepEqual(violations(file => file.primitive ? [] : [
    ...tokens(file.text).filter(token => offScaleHeight.test(token)),
    ...controlTags(file.text).flatMap(tag => tokens(tag).filter(token => {
      const size = numericHeight.exec(token);
      return size !== null && isControlHeight(Number(size[1]));
    })),
  ]), []);
});
