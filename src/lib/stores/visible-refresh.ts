import { windowActive } from "./window-activity";

export function startVisibleRefresh(refresh: () => void, milliseconds: number): () => void {
  let timer: ReturnType<typeof setInterval> | undefined;
  const stop = windowActive.subscribe((active) => {
    clearInterval(timer);
    timer = undefined;
    if (active) {
      refresh();
      timer = setInterval(refresh, milliseconds);
    }
  });
  return () => { stop(); clearInterval(timer); };
}
