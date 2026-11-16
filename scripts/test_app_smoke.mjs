import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { test } from "node:test";
import { compareWithBudget, hasRegression, summaryMarkdown } from "../tools/app-smoke/budget.mjs";
import { descendants, parseKibibytes, parseProcStat } from "../tools/app-smoke/processes.mjs";

test("proc stat parsing survives spaces and parentheses in the command name", () => {
  // Fields: pid (comm) state ppid ... utime=14 stime=15.
  const line = "4242 (Web Content (x)) S 4200 4242 4200 0 -1 4194560 100 0 0 0 37 5 0 0 20 0 12 0 9000 1000 2000 0 0";
  assert.deepEqual(parseProcStat(line), { pid: 4242, ppid: 4200, cpuTicks: 42 });
});

test("memory fields are read in kibibytes", () => {
  const rollup = "Rss:              20480 kB\nPss:              10240 kB\nPss_Anon:          5000 kB\n";
  assert.equal(parseKibibytes(rollup, "Pss"), 10240);
  assert.equal(parseKibibytes(rollup, "Swap"), null);
});

test("descendants include grandchildren but not siblings", () => {
  const processes = [
    { pid: 10, ppid: 1 }, { pid: 11, ppid: 10 }, { pid: 12, ppid: 11 }, { pid: 20, ppid: 1 },
  ];
  assert.deepEqual(descendants(processes, 10).map(({ pid }) => pid).sort(), [11, 12]);
});

test("only measurements above the platform budget fail", () => {
  const budget = { platforms: { linux: { startupMs: { max: 1000 }, idleMemoryMiB: { max: 500 } } } };
  const rows = compareWithBudget({ startupMs: 1200, idleMemoryMiB: 400, hiddenCpuPercent: 1 }, budget, "linux");
  assert.deepEqual(rows.map(({ id, status }) => [id, status]), [
    ["startupMs", "over budget"], ["idleMemoryMiB", "ok"], ["hiddenCpuPercent", "no budget"],
  ]);
  assert.equal(hasRegression(rows), true);
  const unmeasured = compareWithBudget({ startupMs: 900 }, budget, "linux");
  assert.equal(unmeasured[1].status, "not measured");
  assert.equal(hasRegression(unmeasured), false);
  assert.equal(hasRegression(compareWithBudget({ startupMs: 9000 }, budget, "darwin")), false);
});

test("summary lists every metric and flags a provisional budget", () => {
  const rows = compareWithBudget({ startupMs: 1234 }, { platforms: { linux: { startupMs: { max: 2000 } } } }, "linux");
  const summary = summaryMarkdown(rows, { platform: "linux", build: "debug", provisional: true });
  assert.match(summary, /\| Cold start to first rendered window \| 1234 ms \| 2000 ms \| ok \|/);
  assert.match(summary, /not measured/);
  assert.match(summary, /provisional/);
});

test("checked-in budget covers every metric on Linux", async () => {
  const budget = JSON.parse(await readFile(new URL("../tools/app-smoke/performance-budget.json", import.meta.url), "utf8"));
  for (const row of compareWithBudget({ startupMs: 0, idleMemoryMiB: 0, hiddenCpuPercent: 0 }, budget, "linux")) {
    assert.equal(row.status, "ok", row.id);
  }
});
