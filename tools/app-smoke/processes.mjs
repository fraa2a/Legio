import { execFileSync } from "node:child_process";
import { readdirSync, readFileSync, readlinkSync, realpathSync } from "node:fs";

// Fields after the parenthesised command name, which may itself contain spaces.
export function parseProcStat(text) {
  const end = text.lastIndexOf(")");
  if (end < 0) throw new Error("Malformed /proc stat line");
  const pid = Number.parseInt(text.slice(0, text.indexOf("(")), 10);
  const fields = text.slice(end + 2).trim().split(/\s+/);
  return {
    pid,
    ppid: Number.parseInt(fields[1], 10),
    cpuTicks: Number.parseInt(fields[11], 10) + Number.parseInt(fields[12], 10),
  };
}

// Returns kibibytes for a "Name:   123 kB" line, or null when the field is missing.
export function parseKibibytes(text, field) {
  const match = new RegExp(`^${field}:\\s+(\\d+) kB$`, "m").exec(text);
  return match === null ? null : Number.parseInt(match[1], 10);
}

function readText(path) {
  try {
    return readFileSync(path, "utf8");
  } catch {
    // The process exited between listing and reading.
    return null;
  }
}

function linuxProcesses() {
  const processes = [];
  for (const entry of readdirSync("/proc")) {
    if (!/^\d+$/.test(entry)) continue;
    const stat = readText(`/proc/${entry}/stat`);
    if (stat !== null) processes.push(parseProcStat(stat));
  }
  return processes;
}

export function descendants(processes, root) {
  const children = new Map();
  for (const process of processes) {
    const list = children.get(process.ppid) ?? [];
    list.push(process);
    children.set(process.ppid, list);
  }
  const result = [];
  const pending = [root];
  while (pending.length > 0) {
    for (const child of children.get(pending.pop()) ?? []) {
      result.push(child);
      pending.push(child.pid);
    }
  }
  return result;
}

function linuxExecutable(pid) {
  try {
    return readlinkSync(`/proc/${pid}/exe`);
  } catch {
    return null;
  }
}

export function findApplicationPid(driverPid, application) {
  if (process.platform === "linux") {
    const target = realpathSync(application);
    const match = descendants(linuxProcesses(), driverPid).find((entry) => linuxExecutable(entry.pid) === target);
    return match?.pid ?? null;
  }
  if (process.platform === "win32") {
    const name = application.split(/[\\/]/).pop();
    const output = execFileSync("tasklist", ["/FI", `IMAGENAME eq ${name}`, "/FO", "CSV", "/NH"], { encoding: "utf8" });
    const pids = output.split(/\r?\n/).map((line) => /^"[^"]*","(\d+)"/.exec(line)?.[1]).filter(Boolean);
    return pids.length === 1 ? Number.parseInt(pids[0], 10) : null;
  }
  return null;
}

export function isRunning(pid) {
  try {
    process.kill(pid, 0);
    return true;
  } catch (error) {
    return error.code === "EPERM";
  }
}

// The application and its WebKit helper processes; Linux only.
function applicationTree(pid) {
  return [{ pid }, ...descendants(linuxProcesses(), pid)];
}

export function sampleMemory(pid) {
  let pss = 0;
  let rss = 0;
  const processes = applicationTree(pid);
  for (const { pid: member } of processes) {
    const rollup = readText(`/proc/${member}/smaps_rollup`);
    const status = readText(`/proc/${member}/status`);
    rss += (status === null ? null : parseKibibytes(status, "VmRSS")) ?? 0;
    pss += (rollup === null ? null : parseKibibytes(rollup, "Pss")) ?? 0;
  }
  return { processes: processes.length, pssMiB: pss / 1024, rssMiB: rss / 1024 };
}

function cpuTicks(pid) {
  let total = 0;
  for (const { pid: member } of applicationTree(pid)) {
    const stat = readText(`/proc/${member}/stat`);
    if (stat !== null) total += parseProcStat(stat).cpuTicks;
  }
  return total;
}

// Average CPU use of the process tree, in percent of one core.
export async function sampleCpu(pid, seconds) {
  const ticksPerSecond = Number.parseInt(execFileSync("getconf", ["CLK_TCK"], { encoding: "utf8" }), 10);
  const startTicks = cpuTicks(pid);
  const start = performance.now();
  await new Promise((resolve) => setTimeout(resolve, seconds * 1000));
  const elapsed = (performance.now() - start) / 1000;
  return ((cpuTicks(pid) - startTicks) / ticksPerSecond / elapsed) * 100;
}
