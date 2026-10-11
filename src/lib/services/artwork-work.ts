import { invoke } from "@tauri-apps/api/core";
import { logApplicationEvent } from "./application-log";

export type ArtworkWork = { requestId: string; origin: "visible" | "decorative" | "prefetch"; release(): void };
export function createArtworkWork(_appId: number, origin: ArtworkWork["origin"]): ArtworkWork {
  const requestId = crypto.randomUUID();
  let released = false;
  return { requestId, origin, release() {
    if (released) return;
    released = true;
    void invoke("cancel_artwork_request", { requestId }).catch(() => logApplicationEvent("warn", "command_error", "Artwork cancellation unavailable"));
  } };
}
