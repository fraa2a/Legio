import { mount } from "svelte";
import App from "./App.svelte";
import "./app.css";
import { settings } from "./lib/stores/settings";
import { setLanguage } from "./lib/i18n";

const target = document.getElementById("app");

if (!target) {
  throw new Error("Missing application mount point");
}

async function start(): Promise<void> {
  await settings.load();
  settings.subscribe((state) => setLanguage(state.data.language));
  mount(App, { target: target! });
}

void start();
