import { invoke } from "@tauri-apps/api/core";

export type ApplicationLogLevel = "info" | "warn" | "error";
export type ApplicationLogEvent = "renderer_start" | "renderer_ready" | "renderer_error" | "unhandled_rejection" | "console" | "navigation" | "command_error" | "bootstrap_error";

export interface ApplicationLogStatus {
  enabled: boolean;
  directory: string | null;
  lastError: string | null;
  droppedRecords: number;
}

// Before settings load, the native logger enforces the saved opt-in.
let enabled: boolean | undefined;
let windowStart = 0;
let recordCount = [0, 0];
let warned = false;
let warningOutput = console.warn.bind(console);

export function setApplicationLoggingEnabled(value: boolean): void {
  if (enabled !== value) {
    windowStart = 0;
    recordCount = [0, 0];
  }
  enabled = value;
}

function bounded(value: string, limit: number): string {
  const bytes = new TextEncoder().encode(value);
  return bytes.length <= limit ? value : new TextDecoder().decode(bytes.subarray(0, limit), { stream: true });
}

export function describeApplicationError(error: unknown): string {
  if (error instanceof Error) return `${error.name}: ${error.message}`;
  return typeof error === "string" ? error : "Non-Error failure";
}

export function applicationErrorStack(error: unknown): string | null {
  return error instanceof Error ? error.stack ?? null : null;
}

export function logApplicationEvent(level: ApplicationLogLevel, event: ApplicationLogEvent, message = "", stack: string | null = null): void {
  if (enabled === false) return;
  const now = Date.now();
  if (now - windowStart >= 60_000) {
    windowStart = now;
    recordCount = [0, 0];
  }
  const bucket = event === "renderer_error" || event === "unhandled_rejection" || event === "bootstrap_error" ? 1 : 0;
  if (recordCount[bucket] >= 50) return;
  recordCount[bucket]++;
  void invoke("report_application_event", {
    level, event, message: bounded(message, 1024), stack: stack === null ? null : bounded(stack, 4096),
  }).catch(() => {
    if (warned) return;
    warned = true;
    warningOutput("Application log reporting unavailable; check diagnostics settings.");
  });
}

export function getApplicationLogStatus(): Promise<ApplicationLogStatus> {
  return invoke<ApplicationLogStatus>("get_application_log_status");
}

export function installApplicationLogging(): () => void {
  const previousWarn = console.warn;
  const previousError = console.error;
  warningOutput = previousWarn.bind(console);
  const error = (event: ErrorEvent) => {
    logApplicationEvent("error", "renderer_error", `line: ${event.lineno}; column: ${event.colno}; ${describeApplicationError(event.error ?? event.message)}`, applicationErrorStack(event.error));
  };
  const rejection = (event: PromiseRejectionEvent) => {
    logApplicationEvent("error", "unhandled_rejection", describeApplicationError(event.reason), applicationErrorStack(event.reason));
  };
  const forward = (level: "warn" | "error", args: unknown[]) => {
    const failure = args.find(value => value instanceof Error);
    const message = args.slice(0, 4).map(describeApplicationError).join("; ");
    logApplicationEvent(level, "console", message, applicationErrorStack(failure));
  };
  console.warn = (...args: unknown[]) => { previousWarn.apply(console, args); forward("warn", args); };
  console.error = (...args: unknown[]) => { previousError.apply(console, args); forward("error", args); };
  window.addEventListener("error", error);
  window.addEventListener("unhandledrejection", rejection);
  return () => {
    window.removeEventListener("error", error);
    window.removeEventListener("unhandledrejection", rejection);
    console.warn = previousWarn;
    console.error = previousError;
  };
}
