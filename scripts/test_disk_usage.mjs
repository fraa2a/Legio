import assert from "node:assert/strict";
import { test } from "node:test";
import { buildSizeRows, totalBytes, usageSegments } from "../src/lib/features/settings/disk-usage-model.ts";

const game = (id, name, extra = {}) => ({ id, name, steamAppId: null, steamInstallPath: null, executablePath: `/games/${name}/run`, installationRoot: `/games/${name}`, ...extra });
const size = (gameId, bytes, partial = false) => ({ gameId, bytes, partial, measuredAt: 1 });

test("rows list the largest game first and unmeasured games last by name", () => {
  const rows = buildSizeRows(
    [game("a", "Alpha"), game("b", "Beta"), game("c", "Gamma"), game("d", "Delta"), game("e", "Epsilon")],
    [size("a", 10), size("b", 500), size("e", 10)],
    [],
    [],
  );
  assert.deepEqual(rows.map((row) => row.game.id), ["b", "a", "e", "d", "c"]);
  assert.deepEqual(rows.map((row) => row.bytes), [500, 10, 10, null, null]);
});

test("games without an installation folder are left out", () => {
  const rows = buildSizeRows([game("a", "Alpha"), game("b", "Beta", { installationRoot: null })], [], [], []);
  assert.deepEqual(rows.map((row) => row.game.id), ["a"]);
});

test("rows carry library, last played, partial flag and the uninstall rule", () => {
  const rows = buildSizeRows(
    [game("a", "Alpha"), game("s", "Steam", { steamInstallPath: "/steam/Steam", installationRoot: null }), game("n", "Manual", { executablePath: null })],
    [size("a", 7, true)],
    [{ id: "lib", root: "/games", warning: null, isDefault: true, freeBytes: 1, games: [{ id: "a", name: "Alpha" }] }],
    [{ gameId: "a", totalMilliseconds: 1, activeSessions: 0, lastPlayedAt: 99 }],
  );
  const byId = Object.fromEntries(rows.map((row) => [row.game.id, row]));
  assert.equal(byId.a.library, "/games");
  assert.equal(byId.a.lastPlayedAt, 99);
  assert.equal(byId.a.partial, true);
  assert.equal(byId.a.removable, true);
  assert.equal(byId.s.removable, false);
  assert.equal(byId.n.removable, false);
  assert.equal(byId.s.library, null);
});

test("the total counts only measured games", () => {
  const rows = buildSizeRows([game("a", "Alpha"), game("b", "Beta")], [size("a", 40)], [], []);
  assert.equal(totalBytes(rows), 40);
});

const usage = (extra) => ({ libraryId: "lib", totalBytes: 1000, freeBytes: 300, legioBytes: 200, otherBytes: 500, estimated: false, ...extra });

test("usage segments add up to the whole disk", () => {
  assert.deepEqual(usageSegments(usage({})), { legio: 20, other: 50, free: 30 });
});

test("usage segments clamp when the numbers disagree and hide when the disk is unknown", () => {
  const over = usageSegments(usage({ legioBytes: 900, otherBytes: 900 }));
  assert.equal(over.legio, 90);
  assert.equal(over.other, 10);
  assert.equal(over.free, 0);
  assert.equal(usageSegments(usage({ totalBytes: null, freeBytes: null, otherBytes: null })), null);
  assert.equal(usageSegments(usage({ totalBytes: 0 })), null);
});
