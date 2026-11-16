import assert from "node:assert/strict";
import { test } from "node:test";
import {
  chartBars,
  dayLabels,
  defaultRecapSelection,
  formatRecapDate,
  formatRecapPeriod,
  formatShare,
  monthNames,
  recapBoundaries,
  recapFileName,
  showsDayLabel,
  weekdayNames,
} from "../src/lib/features/recap/recap-model.ts";
import { drawRecapCard, fitText, recapCardSize } from "../src/lib/features/recap/recap-card.ts";

function withTimezone(zone, run) {
  const previous = process.env.TZ;
  process.env.TZ = zone;
  try {
    run();
  } finally {
    if (previous === undefined) delete process.env.TZ;
    else process.env.TZ = previous;
  }
}

test("boundaries cover every local day of a year or month", () => {
  assert.equal(recapBoundaries({ mode: "year", year: 2024, month: 1 }).length, 367);
  assert.equal(recapBoundaries({ mode: "year", year: 2025, month: 1 }).length, 366);
  assert.equal(recapBoundaries({ mode: "month", year: 2024, month: 2 }).length, 30);
  assert.equal(recapBoundaries({ mode: "month", year: 2025, month: 12 }).length, 32);
});

test("boundaries follow local midnights across daylight saving", () => {
  withTimezone("Europe/Rome", () => {
    const march = recapBoundaries({ mode: "month", year: 2026, month: 3 });
    assert.equal(march[29] - march[28], 23 * 3600000);
    const year = recapBoundaries({ mode: "year", year: 2026, month: 1 });
    assert.equal(year.length, 366);
    assert.ok(year.slice(1).every((boundary, index) => boundary - year[index] >= 23 * 3600000 && boundary - year[index] <= 25 * 3600000));
  });
});

test("the default period is the current year, else the newest year with data", () => {
  const now = new Date(2026, 4, 10);
  assert.deepEqual(defaultRecapSelection([2026, 2024], now), { mode: "year", year: 2026, month: 5 });
  assert.deepEqual(defaultRecapSelection([2024, 2022], now), { mode: "year", year: 2024, month: 12 });
});

test("file names and titles identify the period", () => {
  assert.equal(recapFileName({ mode: "year", year: 2025, month: 3 }), "legio-recap-2025.png");
  assert.equal(recapFileName({ mode: "month", year: 2025, month: 3 }), "legio-recap-2025-03.png");
  assert.equal(formatRecapPeriod({ mode: "year", year: 2025, month: 3 }, "en"), "2025");
  assert.equal(formatRecapPeriod({ mode: "month", year: 2025, month: 3 }, "en"), "March 2025");
  assert.equal(formatRecapPeriod({ mode: "month", year: 2025, month: 3 }, "it"), "Marzo 2025");
});

test("weekdays start on Monday and months are capitalized in both languages", () => {
  assert.deepEqual(weekdayNames("en", "short"), ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"]);
  assert.equal(weekdayNames("it", "long")[0], "Lunedì");
  assert.equal(monthNames("it", "long")[2], "Marzo");
  assert.equal(monthNames("en", "short")[11], "Dec");
});

test("calendar dates are formatted without a time zone shift", () => {
  withTimezone("America/Los_Angeles", () => {
    assert.equal(formatRecapDate("2024-03-01", "en", { day: "numeric", month: "long" }), "March 1");
    assert.equal(formatRecapDate("2024-12-31", "it", { day: "numeric", month: "long", year: "numeric" }), "31 dicembre 2024");
  });
});

test("shares round to whole percents and small ones stay visible", () => {
  assert.equal(formatShare(0.5, "en"), "50%");
  assert.equal(formatShare(1 / 3, "it"), "33%");
  assert.equal(formatShare(0.004, "en"), "<1%");
  assert.equal(formatShare(0, "en"), "0%");
});

test("bars scale to the tallest value and keep a sliver for nonzero days", () => {
  const bars = chartBars([0, 100, 1, 50], ["A", "B", "C", "D"], (value) => `${value} ms`);
  assert.deepEqual(bars.map((bar) => bar.percent), [0, 100, 2, 50]);
  assert.equal(bars[1].text, "B: 100 ms");
  assert.deepEqual(chartBars([0, 0], ["A", "B"], String).map((bar) => bar.percent), [0, 0]);
});

test("day labels skip crowded positions", () => {
  assert.equal(dayLabels(30).length, 30);
  const shown = Array.from({ length: 31 }, (_, index) => index + 1).filter((day) => showsDayLabel(day, 31));
  assert.deepEqual(shown, [1, 5, 10, 15, 20, 25]);
});

test("long names are cut to fit the card", () => {
  const context = { measureText: (text) => ({ width: text.length * 10 }) };
  assert.equal(fitText(context, "Short", 100), "Short");
  const fitted = fitText(context, "A very long game title", 120);
  assert.ok(fitted.endsWith("..."));
  assert.ok(context.measureText(fitted).width <= 120);
});

test("the card draws every game inside its fixed size", () => {
  const calls = [];
  const context = new Proxy({}, {
    get: (_, name) => name === "measureText" ? (text) => ({ width: text.length * 10 }) : (...args) => calls.push([name, ...args]),
    set: (_, name, value) => { calls.push([`set:${name}`, value]); return true; },
  });
  const colors = { background: "#000", surface: "#111", text: "#fff", muted: "#999", accent: "#0af" };
  drawRecapCard(context, {
    brand: "Legio",
    heading: "Play recap",
    period: "2025",
    stats: [{ label: "Play time", value: "10 h" }, { label: "Games played", value: "3" }],
    topTitle: "Most played games",
    games: [{ name: "Alpha", duration: "6 h", share: 0.6, shareLabel: "60%", image: null }, { name: "Beta", duration: "4 h", share: 0.4, shareLabel: "40%", image: null }],
    colors,
  });
  const text = calls.filter(([name]) => name === "fillText").map(([, value]) => value);
  assert.ok(["2025", "Alpha", "Beta", "10 h"].every((value) => text.includes(value)));
  assert.ok(calls.some(([name, , , width, height]) => name === "fillRect" && width === recapCardSize.width && height === recapCardSize.height));
});
