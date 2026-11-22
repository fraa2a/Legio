import { mount } from "svelte";
import "./app.css";
import { applicationErrorStack, describeApplicationError, installApplicationLogging, logApplicationEvent } from "./lib/services/application-log";

const stopLogging = installApplicationLogging();
if (import.meta.hot) import.meta.hot.dispose(stopLogging);
logApplicationEvent("info", "renderer_start", "Renderer starting");

const target = document.getElementById("app");

if (!target) {
  throw new Error("Missing application mount point");
}

async function start(): Promise<void> {
  const [{ default: App }, { settings }, { appInfo }, { setLanguage, setSystemLanguage }] = await Promise.all([
    import("./App.svelte"), import("./lib/stores/settings"), import("./lib/stores/app-info"), import("./lib/i18n"),
  ]);
  await Promise.all([settings.load(), appInfo.load()]);
  appInfo.subscribe((state) => setSystemLanguage(state.data.systemLanguage));
  settings.subscribe((state) => setLanguage(state.data.language));
  mount(App, { target: target! });
  logApplicationEvent("info", "renderer_ready", "Application mounted");
}

void start().catch(error => {
  logApplicationEvent("error", "bootstrap_error", describeApplicationError(error), applicationErrorStack(error));
  target.textContent = "Legio could not start. Restart the application and check the application log if enabled.";
  target.setAttribute("role", "alert");
});
