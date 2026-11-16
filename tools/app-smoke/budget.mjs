export const METRICS = {
  startupMs: { label: "Cold start to first rendered window", unit: "ms" },
  idleMemoryMiB: { label: "Proportional memory at idle (app and WebKit processes)", unit: "MiB" },
  hiddenCpuPercent: { label: "CPU while hidden in the tray (percent of one core)", unit: "%" },
};

// Compares measured metrics with the platform budget. Missing measurements are reported, not failed.
export function compareWithBudget(metrics, budget, platform) {
  const limits = budget.platforms?.[platform] ?? {};
  return Object.entries(METRICS).map(([id, { label, unit }]) => {
    const value = metrics[id] ?? null;
    const max = limits[id]?.max ?? null;
    const status = value === null ? "not measured" : max === null ? "no budget" : value <= max ? "ok" : "over budget";
    return { id, label, unit, value, max, status };
  });
}

export function hasRegression(rows) {
  return rows.some((row) => row.status === "over budget");
}

function format(value) {
  return value === null ? "-" : Number.isInteger(value) ? String(value) : value.toFixed(1);
}

export function summaryMarkdown(rows, { platform, build, provisional }) {
  const lines = [
    `### Performance (${platform}, ${build} build)`,
    "",
    "| Metric | Value | Budget | Status |",
    "| --- | ---: | ---: | --- |",
    ...rows.map((row) => `| ${row.label} | ${format(row.value)} ${row.unit} | ${format(row.max)} ${row.unit} | ${row.status} |`),
  ];
  if (provisional) lines.push("", "The budget is provisional; see tools/app-smoke/README.md to update it.");
  return `${lines.join("\n")}\n`;
}
