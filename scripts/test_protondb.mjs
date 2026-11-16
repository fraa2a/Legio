import assert from "node:assert/strict";
import { test } from "node:test";
import { createModuleLoader } from "./test_module_loader.mjs";

const { load } = createModuleLoader({});
const links = await import(await load("src/lib/features/library/game-links.ts"));
const model = await import(await load("src/lib/features/library/protondb.ts"));

const sources = (overrides = {}) => ({ steamAppId: 620, thunderstoreCommunity: null, linux: false, ...overrides });
const urls = (list) => Object.fromEntries(list.map((link) => [link.kind, link.url]));

test("builds links from the Steam App ID", () => {
  assert.deepEqual(urls(links.gameLinks(sources())), {
    pcgamingwiki: "https://www.pcgamingwiki.com/api/appid.php?appid=620",
    steam: "https://store.steampowered.com/app/620",
  });
});

test("adds ProtonDB only on Linux", () => {
  assert.equal(urls(links.gameLinks(sources())).protondb, undefined);
  assert.equal(urls(links.gameLinks(sources({ linux: true }))).protondb, "https://www.protondb.com/app/620");
});

test("adds Thunderstore for a known community", () => {
  assert.equal(urls(links.gameLinks(sources({ thunderstoreCommunity: "lethal-company" }))).thunderstore, "https://thunderstore.io/c/lethal-company/");
  assert.deepEqual(links.gameLinks(sources({ steamAppId: null, thunderstoreCommunity: "risk-of-rain-2" })).map((link) => link.kind), ["thunderstore"]);
});

test("shows no links without usable identifiers", () => {
  assert.deepEqual(links.gameLinks(sources({ steamAppId: null, linux: true })), []);
  for (const steamAppId of [0, -5, 1.5, Number.NaN, Number.MAX_SAFE_INTEGER + 2]) {
    assert.deepEqual(links.gameLinks(sources({ steamAppId, linux: true })), [], String(steamAppId));
  }
});

test("rejects a malformed community identifier", () => {
  for (const thunderstoreCommunity of ["", "../x", "a/b", "a b", "https://evil.test", "a?b=1", "-start", "x".repeat(65)]) {
    assert.deepEqual(links.gameLinks(sources({ steamAppId: null, thunderstoreCommunity })), [], thunderstoreCommunity);
  }
});

test("every tier has a label and a meaning", () => {
  const tiers = ["platinum", "gold", "silver", "bronze", "borked", "pending", "unknown"];
  assert.deepEqual(Object.keys(model.tierLabel), tiers);
  assert.deepEqual(Object.keys(model.tierMeaning), tiers);
  assert.equal(model.tierLabel.platinum, "Platino");
  assert.equal(model.tierMeaning.platinum, "Funziona perfettamente senza modifiche");
  assert.deepEqual(Object.keys(model.confidenceLabel), ["strong", "good", "moderate", "low", "inadequate", "unknown"]);
});

test("the badge shows only real ratings", () => {
  const summary = (tier) => ({ tier, confidence: null, score: null, totalReports: 1, trendingTier: null, bestReportedTier: null });
  for (const tier of ["platinum", "gold", "silver", "bronze", "borked"]) assert.equal(model.badgeTier(summary(tier)), tier);
  for (const tier of ["pending", "unknown"]) assert.equal(model.badgeTier(summary(tier)), null);
  assert.equal(model.badgeTier(null), null);
});
