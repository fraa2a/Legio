import { listen } from "@tauri-apps/api/event";
import { writable } from "svelte/store";
import { takeSourceLinks } from "../services/legio-source";
import { toMessage } from "../utils/errors";
import { openSettings } from "./navigation";
import { addSource, sourceActionError } from "./source";

export const sourceLinkRegistrationError = writable<string | null>(null);
let starting: Promise<void> | null = null;
let draining = false;
let requested = false;

async function drain(): Promise<void> {
  requested = true;
  if (draining) return;
  draining = true;
  try {
    while (requested) {
      requested = false;
      const pending = await takeSourceLinks();
      sourceLinkRegistrationError.set(pending.registrationError);
      for (const request of pending.requests) {
        openSettings("sources");
        if (request.error !== null) sourceActionError.set(request.error);
        else if (request.url !== null) await addSource(request.url);
      }
    }
  } catch (error) {
    openSettings("sources");
    sourceActionError.set(toMessage(error));
  } finally {
    draining = false;
  }
}

export function startSourceLinks(): Promise<void> {
  if (starting !== null) return starting;
  starting = (async () => {
    await listen("legio:source-link", () => { void drain(); });
    await drain();
  })().catch((error) => {
    starting = null;
    sourceLinkRegistrationError.set(toMessage(error));
  });
  return starting;
}
