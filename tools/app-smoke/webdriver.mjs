// Minimal W3C WebDriver client for tauri-driver; covers only the commands the smoke test uses.
const ELEMENT = "element-6066-11e4-a52e-4f735466cecf";

export class WebDriverError extends Error {
  constructor(command, status, body) {
    super(`${command} failed (${status}): ${body?.value?.message ?? JSON.stringify(body)}`);
    this.name = "WebDriverError";
    this.webdriverError = body?.value?.error ?? null;
  }
}

async function request(base, method, path, body) {
  const response = await fetch(`${base}${path}`, {
    method,
    headers: body === undefined ? undefined : { "Content-Type": "application/json" },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  const text = await response.text();
  let parsed;
  try {
    parsed = text.length > 0 ? JSON.parse(text) : {};
  } catch {
    throw new WebDriverError(`${method} ${path}`, response.status, { value: { message: text.slice(0, 500) } });
  }
  if (!response.ok) throw new WebDriverError(`${method} ${path}`, response.status, parsed);
  return parsed.value;
}

export class Session {
  constructor(base, id) {
    this.base = base;
    this.id = id;
  }

  static async create(base, capabilities) {
    const value = await request(base, "POST", "/session", { capabilities: { alwaysMatch: capabilities } });
    return new Session(base, value.sessionId);
  }

  command(method, path, body) {
    return request(this.base, method, `/session/${this.id}${path}`, body);
  }

  // Runs `body` as a function body; element references in the result become element ids.
  execute(body, ...args) {
    return this.command("POST", "/execute/sync", { script: body, args });
  }

  executeAsync(body, ...args) {
    return this.command("POST", "/execute/async", { script: body, args });
  }

  click(element) {
    return this.command("POST", `/element/${element[ELEMENT]}/click`, {});
  }

  clear(element) {
    return this.command("POST", `/element/${element[ELEMENT]}/clear`, {});
  }

  sendKeys(element, text) {
    return this.command("POST", `/element/${element[ELEMENT]}/value`, { text });
  }

  setWindowRect(rect) {
    return this.command("POST", "/window/rect", rect);
  }

  async screenshot() {
    return Buffer.from(await this.command("GET", "/screenshot"), "base64");
  }

  delete() {
    return request(this.base, "DELETE", `/session/${this.id}`);
  }
}

export async function waitForServer(base, timeoutMs) {
  const deadline = Date.now() + timeoutMs;
  let last = null;
  while (Date.now() < deadline) {
    try {
      const response = await fetch(`${base}/status`);
      if (response.ok) return;
      last = new Error(`status ${response.status}`);
    } catch (error) {
      last = error;
    }
    await new Promise((resolve) => setTimeout(resolve, 200));
  }
  throw new Error(`WebDriver server at ${base} did not start: ${last?.message ?? "timeout"}`);
}
