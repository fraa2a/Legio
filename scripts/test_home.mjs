import assert from "node:assert/strict";
import { test } from "node:test";
import { activityIntensity, monthCalendar, recentGames } from "../src/lib/features/home/home-model.ts";

test("calendar leaves days outside the month blank and expands to six weeks", () => {
  const october = monthCalendar(2026, 9);
  assert.deepEqual(october.cells.slice(0, 4), [null, null, null, 1]);
  assert.equal(october.cells.length, 35);
  assert.equal(october.cells[34], null);
  const august = monthCalendar(2026, 7);
  assert.equal(august.cells.length, 42);
  assert.equal(august.cells[35], 31);
  assert.deepEqual(august.cells.slice(36), Array(6).fill(null));
  assert.equal(monthCalendar(2024, 1).boundaries.length, 30);
});

test("calendar uses local midnights across daylight saving changes", () => {
  const previousTimezone = process.env.TZ;
  process.env.TZ = "Europe/Rome";
  try {
    const march = monthCalendar(2026, 2);
    assert.equal(march.boundaries[29] - march.boundaries[28], 23 * 3600000);
    const october = monthCalendar(2026, 9);
    assert.equal(october.boundaries[25] - october.boundaries[24], 25 * 3600000);
  } finally {
    if (previousTimezone === undefined) delete process.env.TZ;
    else process.env.TZ = previousTimezone;
  }
});

test("recent games select the last played installation and exclude unplayed games", () => {
  const games = [
    { id: "steam", name: "Game", steamAppId: 42 },
    { id: "manual", name: "Game copy", steamAppId: 42 },
    { id: "other", name: "Other", steamAppId: null },
    { id: "unplayed", name: "Unplayed", steamAppId: null },
  ];
  const summaries = [
    { gameId: "steam", lastPlayedAt: 100 },
    { gameId: "manual", lastPlayedAt: 300 },
    { gameId: "other", lastPlayedAt: 200 },
    { gameId: "unplayed", lastPlayedAt: null },
    { gameId: "deleted", lastPlayedAt: 400 },
  ];
  assert.deepEqual(recentGames(games, summaries).map(({ game }) => game.id), ["manual", "other"]);
});

test("activity intensity increases continuously and saturates at eight hours", () => {
  assert.equal(activityIntensity(0), 0);
  assert.equal(activityIntensity(3 * 3600000), 0.375);
  assert.equal(activityIntensity(4 * 3600000), 0.5);
  assert.equal(activityIntensity(8 * 3600000), 1);
  assert.equal(activityIntensity(12 * 3600000), 1);
});
