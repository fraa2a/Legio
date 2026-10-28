import assert from "node:assert/strict";
import { test } from "node:test";
import { rankPaletteItems } from "../src/lib/features/palette/palette-model.ts";

const item = (id, label, extra = {}) => ({ id, kind: "game", label, ...extra });
const items = [
  item("a", "Hollow Knight"),
  item("b", "Knight's Quest"),
  item("c", "Portal 2"),
  item("d", "Aspetto", { kind: "setting", keywords: "tema palette colori" }),
  item("e", "Città dei Giochi"),
];

test("an empty query keeps the given order and respects the limit", () => {
  assert.deepEqual(rankPaletteItems(items, "  ").map((entry) => entry.id), ["a", "b", "c", "d", "e"]);
  assert.deepEqual(rankPaletteItems(items, "", 2).map((entry) => entry.id), ["a", "b"]);
});

test("prefix matches beat word starts, which beat substrings", () => {
  assert.deepEqual(rankPaletteItems(items, "knight").map((entry) => entry.id), ["b", "a"]);
  assert.deepEqual(rankPaletteItems(items, "ollow").map((entry) => entry.id), ["a"]);
  assert.deepEqual(rankPaletteItems([item("x", "Subnautica"), item("y", "Nautical Sub")], "sub").map((entry) => entry.id), ["x", "y"]);
});

test("matching ignores case and accents and also reads keywords", () => {
  assert.deepEqual(rankPaletteItems(items, "CITTA").map((entry) => entry.id), ["e"]);
  assert.deepEqual(rankPaletteItems(items, "palette").map((entry) => entry.id), ["d"]);
});

test("scattered letters match as a last resort and misses are dropped", () => {
  assert.deepEqual(rankPaletteItems(items, "hkn").map((entry) => entry.id), ["a"]);
  assert.deepEqual(rankPaletteItems(items, "zzz"), []);
});

test("equal ranks keep their original order", () => {
  const same = [item("1", "Alpha One"), item("2", "Alpha Two"), item("3", "Alpha Three")];
  assert.deepEqual(rankPaletteItems(same, "alpha").map((entry) => entry.id), ["1", "2", "3"]);
});
