import { get } from "svelte/store";
import { artworkDisplayWidth } from "../stores/artwork-display";
import { onWindowResized } from "./window";
import { windowActive } from "../stores/window-activity";
import { invoke } from "./invoke";

export function getArtworkDisplayWidth(): Promise<number> {
  return invoke<number>("get_artwork_display_width");
}

export function startArtworkDisplayTracking(): () => void {
  let disposed = false;
  let pending = false;
  let queued = false;
  let timer: ReturnType<typeof setTimeout> | undefined;
  let stopResize: (() => void) | undefined;
  const refresh = async () => {
    if (disposed) return;
    if (pending) { queued = true; return; }
    pending = true;
    try {
      const width = await getArtworkDisplayWidth();
      if (!disposed && width !== get(artworkDisplayWidth)) artworkDisplayWidth.set(width);
    } catch (error) {
      console.warn("Could not refresh artwork display resolution", error);
    } finally {
      pending = false;
      if (queued && !disposed) { queued = false; void refresh(); }
    }
  };
  const schedule = () => {
    clearTimeout(timer);
    timer = setTimeout(() => { void refresh(); }, 200);
  };
  void refresh();
  const stopActivity = windowActive.subscribe(active => { if (active) schedule(); });
  void onWindowResized(schedule).then(stop => {
    if (disposed) stop();
    else stopResize = stop;
  }).catch(error => console.warn("Could not observe artwork display changes", error));
  return () => {
    disposed = true;
    clearTimeout(timer);
    stopActivity();
    stopResize?.();
  };
}
