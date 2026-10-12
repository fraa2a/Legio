import { derived } from "svelte/store";
import { games } from "../stores/games";
import { selectedGameId, selectedStoreGame, activeSection } from "../stores/navigation";
import { invoke } from "@tauri-apps/api/core";
import { logApplicationEvent } from "./application-log";

export type ArtworkWork = { requestId: string; origin: "visible" | "decorative" | "prefetch"; signal?: AbortSignal; release(): void };
export function createArtworkWork(_appId: number, origin: ArtworkWork["origin"]): ArtworkWork {
  const requestId = crypto.randomUUID();
  let released = false;
  const controller = new AbortController();
  return { requestId, origin, signal: controller.signal, release() {
    if (released) return;
    released = true;
    controller.abort();
    void invoke("cancel_artwork_request", { requestId }).catch(() => logApplicationEvent("warn", "command_error", "Artwork cancellation unavailable"));
  } };
}

export function isArtworkCancelled(error: unknown): boolean {
  return error !== null && typeof error === "object" && "kind" in error && error.kind === "cancelled";
}

export function startArtworkSelectionTracking(): () => void {
  let desired: number | null = null;
  let sent: number | null | undefined;
  let running = false;
  let stopped = false;
  async function deliver(): Promise<void> {
    if (running) return;
    running = true;
    try {
      while (desired !== sent) {
        const appId = desired;
        try { await invoke("set_artwork_game", { steamAppId: appId }); }
        catch { logApplicationEvent("warn", "command_error", "Artwork selection update unavailable"); }
        sent = appId;
      }
    } finally { running = false; }
  }
  const selected = derived([activeSection, selectedStoreGame, selectedGameId, games], ([section, store, gameId, library]) => section === "store" ? store?.steamAppId ?? null : section === "library" ? library.data.find(game => game.id === gameId)?.steamAppId ?? null : null);
  const unsubscribe = selected.subscribe(appId => { if (stopped) return; desired = appId; void deliver(); });
  return () => { stopped = true; unsubscribe(); desired = null; void deliver(); };
}
