// Smoke test and performance budget for the built Legio binary, driven through tauri-driver.
// See tools/app-smoke/README.md for usage.
import { spawn } from "node:child_process";
import { chmodSync, existsSync, mkdirSync, mkdtempSync, openSync, readFileSync, rmSync, writeFileSync, appendFileSync } from "node:fs";
import { createServer } from "node:net";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { parseArgs } from "node:util";
import { Session, waitForServer } from "./webdriver.mjs";
import { findApplicationPid, isRunning, sampleCpu, sampleMemory } from "./processes.mjs";
import { compareWithBudget, hasRegression, summaryMarkdown } from "./budget.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const repository = resolve(here, "../..");
const windows = process.platform === "win32";
const linux = process.platform === "linux";
const GAME_NAME = "Smoke Test Game";

const { values: options } = parseArgs({
  options: {
    application: { type: "string" },
    output: { type: "string" },
    "tauri-driver": { type: "string", default: "tauri-driver" },
    "native-driver": { type: "string" },
    budget: { type: "string", default: join(here, "performance-budget.json") },
    "idle-seconds": { type: "string", default: "15" },
    "cpu-seconds": { type: "string", default: "20" },
  },
});

function defaultApplication() {
  const target = process.env.CARGO_TARGET_DIR ?? join(repository, "src-tauri", "target");
  return join(target, "debug", windows ? "legio.exe" : "legio");
}

const application = resolve(options.application ?? defaultApplication());
let output = options.output === undefined ? null : resolve(options.output);
const build = /[\\/]release[\\/]/.test(application) ? "release" : "debug";
const idleSeconds = Number(options["idle-seconds"]);
const cpuSeconds = Number(options["cpu-seconds"]);

function log(message) {
  console.log(`[smoke] ${message}`);
}

function delay(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function freePort() {
  return new Promise((resolve, reject) => {
    const server = createServer();
    server.once("error", reject);
    server.listen(0, "127.0.0.1", () => {
      const { port } = server.address();
      server.close(() => resolve(port));
    });
  });
}

function identifier() {
  return JSON.parse(readFileSync(join(repository, "src-tauri", "tauri.conf.json"), "utf8")).identifier;
}

// Windows resolves application data through known folders, which ignore environment overrides.
// Returns the profile folders this run creates, so they can be removed afterwards.
function claimWindowsProfile() {
  const id = identifier();
  const roots = [process.env.APPDATA, process.env.LOCALAPPDATA].filter(Boolean).map((root) => join(root, id));
  const existing = roots.filter((root) => existsSync(root));
  if (existing.length > 0 && process.env.CI !== "true") {
    throw new Error(`Refusing to run: the smoke test would use the real Legio profile (${existing.join(", ")}). Run it in a disposable Windows account, VM or CI.`);
  }
  return roots.filter((root) => !existing.includes(root));
}

function isolatedEnvironment(root) {
  // Selectors use English labels; LANGUAGE selects them without requiring a generated locale.
  const env = { ...process.env, LANGUAGE: "en", LANG: "C.UTF-8", NO_AT_BRIDGE: "1" };
  delete env.LC_ALL;
  delete env.LC_MESSAGES;
  if (linux) {
    // The System palette follows the GTK preference; dark keeps screenshots consistent.
    env.GTK_THEME ??= "Adwaita:dark";
    const directories = { HOME: "home", XDG_DATA_HOME: "data", XDG_CONFIG_HOME: "config", XDG_CACHE_HOME: "cache", XDG_STATE_HOME: "state", XDG_RUNTIME_DIR: "runtime" };
    for (const [name, directory] of Object.entries(directories)) {
      env[name] = join(root, directory);
      mkdirSync(env[name], { recursive: true, mode: 0o700 });
    }
  }
  return env;
}

// A game executable that is never launched; the importer only checks its type.
function createFixtureGame(root) {
  const directory = join(root, "games", "Smoke Test Game");
  mkdirSync(directory, { recursive: true });
  if (windows) {
    const path = join(directory, "smoke-test-game.exe");
    writeFileSync(path, "MZ");
    return path;
  }
  const path = join(directory, "smoke-test-game");
  writeFileSync(path, "#!/bin/sh\nexit 0\n");
  chmodSync(path, 0o755);
  return path;
}

// A private session bus keeps the single-instance check away from any Legio the user is running.
// It has no service directories, so no portal or other daemon is activated into the profile.
function startSessionBus(root, env, logFile) {
  const config = join(root, "session-bus.conf");
  writeFileSync(config, `<!DOCTYPE busconfig PUBLIC "-//freedesktop//DTD D-Bus Bus Configuration 1.0//EN" "http://www.freedesktop.org/standards/dbus/1.0/busconfig.dtd">
<busconfig>
  <type>session</type>
  <listen>unix:dir=${root}</listen>
  <auth>EXTERNAL</auth>
  <policy context="default">
    <allow send_destination="*" eavesdrop="true"/>
    <allow eavesdrop="true"/>
    <allow own="*"/>
  </policy>
</busconfig>
`);
  return new Promise((resolve, reject) => {
    const bus = spawn("dbus-daemon", [`--config-file=${config}`, "--nofork", "--print-address=1"], { env, stdio: ["ignore", "pipe", logFile] });
    let buffer = "";
    bus.once("error", (error) => reject(new Error(`Could not start dbus-daemon: ${error.message}`)));
    bus.stdout.on("data", (chunk) => {
      buffer += chunk;
      const newline = buffer.indexOf("\n");
      if (newline >= 0) resolve({ process: bus, address: buffer.slice(0, newline).trim() });
    });
    bus.once("exit", (code) => reject(new Error(`dbus-daemon exited with code ${code}`)));
  });
}

// Page helpers prepended to every script executed in the WebView.
const HELPERS = `
const visible = (element) => {
  const rect = element.getBoundingClientRect();
  const style = getComputedStyle(element);
  return rect.width > 0 && rect.height > 0 && style.visibility !== "hidden" && style.display !== "none";
};
const nameOf = (element) => (element.getAttribute("aria-label") ?? element.textContent ?? "").replace(/\\s+/g, " ").trim();
const openDialog = () => [...document.querySelectorAll("dialog[open]")].pop() ?? null;
const scopeOf = (selector) => selector === null ? (openDialog() ?? document) : document.querySelector(selector);
const findButton = (name, selector) => {
  const scope = scopeOf(selector);
  if (scope === null) return null;
  return [...scope.querySelectorAll("button, [role=button], a[href]")].find((element) => visible(element) && nameOf(element) === name) ?? null;
};
`;

class Driver {
  constructor(session) {
    this.session = session;
  }

  run(body, ...args) {
    return this.session.execute(`${HELPERS}\n${body}`, ...args);
  }

  async waitFor(description, body, { timeout = 30000, args = [] } = {}) {
    const deadline = Date.now() + timeout;
    let lastError = null;
    while (Date.now() < deadline) {
      try {
        const value = await this.run(body, ...args);
        if (value) return value;
      } catch (error) {
        lastError = error;
      }
      await delay(250);
    }
    throw new Error(`Timed out waiting for ${description}${lastError === null ? "" : `: ${lastError.message}`}`);
  }

  // `scope` is a CSS selector; by default the topmost open dialog, or the document.
  async clickButton(name, { scope = null, timeout } = {}) {
    const button = await this.waitFor(`button "${name}"`,
      "const button = findButton(arguments[0], arguments[1]); return button !== null && !button.disabled ? button : null;",
      { timeout, args: [name, scope] });
    await this.session.click(button);
  }

  async fill(label, text) {
    const field = await this.waitFor(`field "${label}"`,
      "return [...document.querySelectorAll('input, textarea')].find((input) => [...(input.labels ?? [])].some((item) => nameOf(item) === arguments[0])) ?? null;",
      { args: [label] });
    await this.session.clear(field);
    await this.session.sendKeys(field, text);
  }

  async assertNoErrorBanner(context) {
    const message = await this.run("const scope = openDialog() ?? document; const alert = [...scope.querySelectorAll('[role=alert]')].find(visible); return alert === undefined ? null : nameOf(alert);");
    // CI runners have no Secret Service provider, so the app reports the missing keychain by design.
    if (message !== null && !message.startsWith("No system keychain is available")) throw new Error(`${context} shows an error: ${message}`);
  }

  // Screenshots can return the previous frame, so wait for transitions and fresh frames first.
  async settle() {
    await this.waitFor("transitions to finish",
      "return document.getAnimations().every((animation) => animation.playState !== 'running' || animation.effect?.getTiming().iterations === Infinity);");
    await this.session.executeAsync("const done = arguments[arguments.length - 1]; requestAnimationFrame(() => requestAnimationFrame(() => setTimeout(done, 250)));");
  }

  async capture(name) {
    await this.settle();
    const path = join(output, "screenshots", `${name}.png`);
    writeFileSync(path, await this.session.screenshot());
    log(`captured ${name}`);
  }
}

// Console messages are buffered in the page because WebKitWebDriver has no log endpoint.
const CONSOLE_CAPTURE = `
if (window.__legioSmokeConsole === undefined) {
  const entries = window.__legioSmokeConsole = [];
  const record = (level, values) => {
    if (entries.length >= 1000) entries.shift();
    entries.push(new Date().toISOString() + " " + level + " " + values.map((value) => {
      if (value instanceof Error) return value.stack ?? value.message;
      try { return typeof value === "string" ? value : JSON.stringify(value); } catch { return String(value); }
    }).join(" "));
  };
  for (const level of ["error", "warn", "info", "log"]) {
    const original = console[level].bind(console);
    console[level] = (...values) => { record(level, values); original(...values); };
  }
  window.addEventListener("error", (event) => record("uncaught", [event.error ?? event.message]));
  window.addEventListener("unhandledrejection", (event) => record("unhandled", [event.reason]));
}
`;

// The native file dialog cannot be driven through WebDriver; answer its IPC request with the fixture.
const DIALOG_STUB = `
const path = arguments[0];
if (window.__legioSmokeFetch === undefined) {
  const original = window.__legioSmokeFetch = window.fetch.bind(window);
  window.fetch = (input, init) => {
    const url = typeof input === "string" ? input : input.url;
    if (decodeURIComponent(url).endsWith("/plugin:dialog|open") && window.__legioSmokeDialogPath !== undefined) {
      const answer = window.__legioSmokeDialogPath;
      return Promise.resolve(new Response(JSON.stringify(answer), { headers: { "Content-Type": "application/json", "Tauri-Response": "ok" } }));
    }
    return original(input, init);
  };
}
window.__legioSmokeDialogPath = path;
`;

async function startApplication(base) {
  const started = performance.now();
  const session = await Session.create(base, {
    "tauri:options": { application, args: [] },
  });
  const driver = new Driver(session);
  await driver.waitFor("the application shell",
    "return document.querySelector('.legio-shell') !== null && (document.getElementById('onboarding-title') !== null || document.querySelector('nav[aria-label]') !== null);",
    { timeout: 60000 });
  const startupMs = Math.round(performance.now() - started);
  await driver.run(CONSOLE_CAPTURE);
  return { driver, startupMs };
}

async function completeOnboarding(driver) {
  await driver.waitFor("onboarding", "return document.getElementById('onboarding-title') !== null;");
  await driver.capture("onboarding");
  for (let step = 0; step < 8; step++) {
    const last = await driver.run("return findButton('Start playing', null) !== null;");
    if (last) break;
    await driver.assertNoErrorBanner(`Onboarding step ${step + 1}`);
    await driver.clickButton("Continue");
  }
  await driver.clickButton("Start playing", { timeout: 60000 });
  await driver.waitFor("the main navigation", "return document.querySelector('nav[aria-label=\"Main navigation\"]') !== null;");
}

async function openSection(driver, name) {
  await driver.clickButton(name, { scope: "nav[aria-label=\"Main navigation\"]" });
  await driver.waitFor(`the ${name} section`,
    "const button = findButton(arguments[0], 'nav[aria-label=\"Main navigation\"]'); return button?.getAttribute('aria-current') === 'page';",
    { args: [name] });
}

async function addFixtureGame(driver, executable) {
  await openSection(driver, "Library");
  await driver.clickButton("Add game", { scope: "body" });
  await driver.waitFor("the add game dialog", "return openDialog() !== null;");
  await driver.run(DIALOG_STUB, executable);
  await driver.clickButton("Choose .exe");
  await driver.waitFor("the selected executable", "return openDialog()?.textContent.includes(arguments[0]) === true;", { args: [executable] });
  await driver.fill("Game name", GAME_NAME);
  // Steam identification of the fixture may wait on the network before the button is enabled.
  await driver.clickButton("Add to library", { timeout: 90000 });
  await driver.waitFor("the dialog to close", "return openDialog() === null;");
  await driver.run("window.__legioSmokeDialogPath = undefined;");
  await driver.waitFor("the game in the library", "return findButton(arguments[0], 'main') !== null;", { args: [`Details for ${GAME_NAME}`] });
  await driver.clickButton("Dismiss notification", { scope: "body" });
  await driver.waitFor("the notification to close", "return findButton('Dismiss notification', 'body') === null;");
}

async function checkSettings(driver) {
  await driver.clickButton("Settings", { scope: "nav[aria-label=\"Main navigation\"]" });
  const categories = await driver.waitFor("settings categories",
    "const nav = document.querySelector('dialog[open] nav[aria-label=\"Settings categories\"]'); return nav === null ? null : [...nav.querySelectorAll('button')].map(nameOf);");
  for (const [index, category] of categories.entries()) {
    await driver.clickButton(category, { scope: "dialog[open] nav[aria-label=\"Settings categories\"]" });
    await driver.waitFor(`settings category "${category}"`,
      "return findButton(arguments[0], 'dialog[open] nav')?.getAttribute('aria-current') === 'page';", { args: [category] });
    await delay(500);
    await driver.assertNoErrorBanner(`Settings category "${category}"`);
    if (index === 0) await driver.capture("settings");
    await driver.capture(`settings-${String(index + 1).padStart(2, "0")}`);
  }
  return categories;
}

async function closeSettings(driver) {
  await driver.clickButton("Close settings", { scope: "dialog[open]" });
  await driver.waitFor("settings to close", "return openDialog() === null;");
}

async function setCloseToTray(driver, enabled) {
  await driver.clickButton("Settings", { scope: "nav[aria-label=\"Main navigation\"]" });
  const label = await driver.waitFor("the close to tray toggle",
    "return [...document.querySelectorAll('dialog[open] label')].find((label) => label.querySelector('input[type=checkbox]') !== null && nameOf(label).startsWith('Minimize to tray on close')) ?? null;");
  const checked = await driver.run("return arguments[0].querySelector('input').checked;", label);
  if (checked !== enabled) await driver.session.click(label);
  await driver.waitFor("the toggle to save", "return arguments[0].querySelector('input').checked === arguments[1];", { args: [label, enabled] });
  await closeSettings(driver);
}

// A second launch reaches the running instance, which shows its window again.
function launchSecondInstance(env) {
  return new Promise((resolve, reject) => {
    const child = spawn(application, [], { env, stdio: "ignore" });
    child.once("error", reject);
    child.once("exit", (code) => resolve(code));
  });
}

async function saveConsole(driver) {
  const entries = await driver.run("return window.__legioSmokeConsole ?? [];").catch((error) => [`console capture unavailable: ${error.message}`]);
  writeFileSync(join(output, "console.log"), `${entries.join("\n")}\n`);
}

async function waitForExit(pid, timeoutMs) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    if (!isRunning(pid)) return true;
    await delay(200);
  }
  return false;
}

async function main() {
  if (!existsSync(application)) throw new Error(`Application binary not found: ${application}. Build it with corepack pnpm tauri build --debug --no-bundle.`);
  if (linux && !process.env.DISPLAY && !process.env.WAYLAND_DISPLAY) throw new Error("No display; run under xvfb-run (see tools/app-smoke/README.md).");
  const createdProfile = windows ? claimWindowsProfile() : [];
  output ??= mkdtempSync(join(tmpdir(), "legio-smoke-output-"));
  mkdirSync(join(output, "screenshots"), { recursive: true });
  const root = mkdtempSync(join(tmpdir(), "legio-smoke-"));
  const env = isolatedEnvironment(root);
  const executable = createFixtureGame(root);
  const logFile = openSync(join(output, "driver.log"), "a");
  const results = { platform: process.platform, build, application, steps: [], metrics: {} };
  let bus = null;
  let driverProcess = null;
  let driver = null;
  let failure = null;
  let exited = false;
  const step = async (name, action) => {
    log(name);
    const started = performance.now();
    await action();
    results.steps.push({ name, ms: Math.round(performance.now() - started) });
  };
  try {
    if (linux) {
      bus = await startSessionBus(root, env, logFile);
      env.DBUS_SESSION_BUS_ADDRESS = bus.address;
    }
    const port = await freePort();
    const nativePort = await freePort();
    const driverArgs = ["--port", String(port), "--native-port", String(nativePort)];
    if (options["native-driver"]) driverArgs.push("--native-driver", options["native-driver"]);
    driverProcess = spawn(options["tauri-driver"], driverArgs, { env, stdio: ["ignore", logFile, logFile] });
    const driverExit = new Promise((_, reject) => driverProcess.once("error", reject));
    const base = `http://127.0.0.1:${port}`;
    await Promise.race([waitForServer(base, 30000), driverExit]);

    await step("start application", async () => {
      const started = await startApplication(base);
      driver = started.driver;
      results.metrics.startupMs = started.startupMs;
      await driver.session.setWindowRect({ width: 1280, height: 720 }).catch((error) => log(`could not resize window: ${error.message}`));
    });
    const pid = findApplicationPid(driverProcess.pid, application);
    if (pid === null) throw new Error("Could not find the application process");
    await step("complete onboarding", () => completeOnboarding(driver));
    await step("add a game", () => addFixtureGame(driver, executable));
    await step("capture library", async () => {
      await delay(1000);
      await driver.capture("library");
    });
    await step("open game page", async () => {
      await driver.clickButton(`Details for ${GAME_NAME}`, { scope: "main" });
      await driver.waitFor("the game page", "return document.querySelector('h1')?.textContent.includes(arguments[0]) === true;", { args: [GAME_NAME] });
      await delay(1000);
      await driver.assertNoErrorBanner("Game page");
      await driver.capture("game");
    });
    await step("capture home", async () => {
      await openSection(driver, "Home");
      await delay(1000);
      await driver.assertNoErrorBanner("Home");
      await driver.capture("home");
    });
    await step("capture store", async () => {
      await openSection(driver, "Store");
      await delay(2000);
      await driver.capture("store");
    });
    await step("check settings", async () => {
      results.settingsCategories = await checkSettings(driver);
      await closeSettings(driver);
    });
    await step("measure idle memory", async () => {
      await openSection(driver, "Home");
      await delay(idleSeconds * 1000);
      if (linux) {
        const memory = sampleMemory(pid);
        results.metrics.idleMemoryMiB = Math.round(memory.pssMiB);
        results.memory = memory;
      }
    });
    await step("hide to tray and measure CPU", async () => {
      await setCloseToTray(driver, true);
      await saveConsole(driver);
      await driver.clickButton("Close", { scope: "body" });
      const deadline = Date.now() + 15000;
      let hidden = false;
      while (!hidden && isRunning(pid) && Date.now() < deadline) {
        hidden = await driver.run("return document.visibilityState === 'hidden';").catch(() => false);
        if (!hidden) await delay(250);
      }
      if (!isRunning(pid)) {
        log("no system tray; closing exited the application, hidden CPU not measured");
        exited = true;
        return;
      }
      // WebView2 may keep reporting a visible document for a hidden window, so only Linux enforces it.
      if (!hidden && linux) throw new Error("The window did not hide after Close with close to tray enabled");
      await delay(3000);
      if (linux) results.metrics.hiddenCpuPercent = Number((await sampleCpu(pid, cpuSeconds)).toFixed(2));
      const code = await launchSecondInstance(env);
      if (code !== 0) throw new Error(`Second instance exited with code ${code}`);
      await driver.waitFor("the window to show again", "return document.visibilityState === 'visible';", { timeout: 15000 });
    });
    if (!exited) {
      await step("quit", async () => {
        await setCloseToTray(driver, false);
        await saveConsole(driver);
        // The command can fail because the application exits while handling it.
        await driver.clickButton("Close", { scope: "body" }).catch(() => {});
        if (!(await waitForExit(pid, 20000))) throw new Error("The application did not exit after Close");
        exited = true;
      });
    }
  } catch (error) {
    failure = error;
    console.error(`[smoke] FAILED: ${error.stack ?? error.message}`);
    if (driver !== null && !exited) {
      await driver.session.screenshot().then((image) => writeFileSync(join(output, "failure.png"), image)).catch((reason) => log(`no failure screenshot: ${reason.message}`));
      await saveConsole(driver);
    }
  } finally {
    await driver?.session.delete().catch(() => {});
    driverProcess?.kill();
    bus?.process.kill();
    for (const directory of [root, ...createdProfile]) {
      try {
        rmSync(directory, { recursive: true, force: true, maxRetries: 10, retryDelay: 500 });
      } catch (error) {
        log(`warning: could not remove ${directory}: ${error.message}`);
      }
    }
  }

  const budget = JSON.parse(readFileSync(options.budget, "utf8"));
  const rows = compareWithBudget(results.metrics, budget, process.platform);
  results.budget = rows;
  results.passed = failure === null;
  writeFileSync(join(output, "results.json"), `${JSON.stringify(results, null, 2)}\n`);
  const summary = summaryMarkdown(rows, { platform: process.platform, build, provisional: budget.provisional === true });
  process.stdout.write(`\n${summary}`);
  if (process.env.GITHUB_STEP_SUMMARY) appendFileSync(process.env.GITHUB_STEP_SUMMARY, summary);
  log(`output: ${output}`);
  if (failure !== null) process.exit(1);
  if (hasRegression(rows)) {
    console.error("[smoke] Performance is over budget; see tools/app-smoke/README.md.");
    process.exit(1);
  }
}

await main().catch((error) => {
  console.error(`[smoke] ${error.message}`);
  process.exit(1);
});
