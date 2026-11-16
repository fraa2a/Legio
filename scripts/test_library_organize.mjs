import assert from "node:assert/strict";
import { test } from "node:test";
import {
  collectionMembership,
  filterEntries,
  libraryEntries,
  libraryShelves,
  nextSort,
  sortEntries,
} from "../src/lib/features/library/library-organize.ts";

const game = (id, name, extra = {}) => ({ id, name, steamAppId: null, steamInstallPath: null, ...extra });
const portalSteam = game("portal-steam", "Portal", { steamAppId: 400, steamInstallPath: "/steam/Portal" });
const portalMod = game("portal-mod", "Portal modded", { steamAppId: 400 });
const doom = game("doom", "doom");
const braid = game("braid", "Braid");
const collections = [
  { id: "coop", name: "Co-op", gameIds: ["portal-mod", "doom"] },
  { id: "empty", name: "Empty", gameIds: [] },
];
const organization = new Map([
  ["portal-steam", { gameId: "portal-steam", favorite: false, playStatus: "none", addedAt: 300 }],
  ["portal-mod", { gameId: "portal-mod", favorite: true, playStatus: "playing", addedAt: 100 }],
  ["doom", { gameId: "doom", favorite: false, playStatus: "completed", addedAt: 200 }],
  ["braid", { gameId: "braid", favorite: false, playStatus: "none", addedAt: null }],
]);
const summaries = new Map([
  ["portal-steam", { totalMilliseconds: 10, lastPlayedAt: 50 }],
  ["portal-mod", { totalMilliseconds: 20, lastPlayedAt: 90 }],
  ["doom", { totalMilliseconds: 500, lastPlayedAt: 10 }],
]);
const entries = libraryEntries(
  [
    { game: portalSteam, group: [portalSteam, portalMod], totalMilliseconds: 30 },
    { game: doom, group: [doom], totalMilliseconds: 500 },
    { game: braid, group: [braid], totalMilliseconds: 0 },
  ],
  summaries,
  organization,
  collectionMembership(collections),
);
const names = (list) => list.map((entry) => entry.game.name);
const all = { favoritesOnly: false, playStatus: null, collection: { kind: "all" } };

test("versions of a game share favorite, status, membership and dates", () => {
  const [portal] = entries;
  assert.deepEqual(portal.gameIds, ["portal-steam", "portal-mod"]);
  assert.equal(portal.favorite, true);
  assert.equal(portal.playStatus, "playing", "the shown version has no status, so a set one is used");
  assert.deepEqual([...portal.collectionIds], ["coop"]);
  assert.equal(portal.lastPlayedAt, 90);
  assert.equal(portal.addedAt, 100, "the group counts from its first addition");
  assert.equal(portal.origin, "steam");
  assert.equal(entries[1].origin, "local");
  assert.equal(entries[2].addedAt, null);
});

test("filters combine favorites, status and collection", () => {
  assert.deepEqual(names(filterEntries(entries, { ...all, favoritesOnly: true }, collections)), ["Portal"]);
  assert.deepEqual(names(filterEntries(entries, { ...all, playStatus: "completed" }, collections)), ["doom"]);
  assert.deepEqual(names(filterEntries(entries, { ...all, collection: { kind: "collection", id: "coop" } }, collections)), ["Portal", "doom"]);
  assert.deepEqual(names(filterEntries(entries, { ...all, collection: { kind: "uncollected" } }, collections)), ["Braid"]);
  assert.deepEqual(names(filterEntries(entries, { ...all, collection: { kind: "collection", id: "deleted" } }, collections)).length, 3, "a deleted collection filters nothing");
  assert.deepEqual(names(filterEntries(entries, { favoritesOnly: true, playStatus: "completed", collection: { kind: "all" } }, collections)), []);
});

test("sorts by each key with unknown dates last in both directions", () => {
  assert.deepEqual(names(sortEntries(entries, "name", false)), ["Braid", "doom", "Portal"]);
  assert.deepEqual(names(sortEntries(entries, "name", true)), ["Portal", "doom", "Braid"]);
  assert.deepEqual(names(sortEntries(entries, "playtime", true)), ["doom", "Portal", "Braid"]);
  assert.deepEqual(names(sortEntries(entries, "lastPlayed", true)), ["Portal", "doom", "Braid"]);
  assert.deepEqual(names(sortEntries(entries, "lastPlayed", false)), ["doom", "Portal", "Braid"]);
  assert.deepEqual(names(sortEntries(entries, "added", true)), ["doom", "Portal", "Braid"]);
  assert.deepEqual(names(sortEntries(entries, "added", false)), ["Portal", "doom", "Braid"]);
  assert.deepEqual(names(sortEntries(entries, "origin", false)), ["Portal", "Braid", "doom"], "ties fall back to the name");
  assert.deepEqual(names(entries), ["Portal", "doom", "Braid"], "sorting returns a copy");
});

test("a header click flips the current key and starts a new key in its natural direction", () => {
  assert.deepEqual(nextSort({ sort: "name", descending: false }, "name"), { sort: "name", descending: true });
  assert.deepEqual(nextSort({ sort: "name", descending: false }, "playtime"), { sort: "playtime", descending: true });
  assert.deepEqual(nextSort({ sort: "playtime", descending: true }, "origin"), { sort: "origin", descending: false });
});

test("shelves follow collection order, keep the sort and end with uncollected games", () => {
  const shelves = libraryShelves(sortEntries(entries, "playtime", true), collections, "No collection");
  assert.deepEqual(shelves.map((shelf) => [shelf.id, shelf.name, names(shelf.entries)]), [
    ["coop", "Co-op", ["doom", "Portal"]],
    [null, "No collection", ["Braid"]],
  ]);
});
