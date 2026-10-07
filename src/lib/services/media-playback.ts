export function observeMediaPlayback(subscribe: (listener: (active: boolean) => void) => () => void): () => void {
  let active = false;
  const paused = new Set<HTMLMediaElement>();
  const suspend = (media: HTMLMediaElement) => {
    if (!media.paused && !media.ended) {
      paused.add(media);
      media.pause();
    }
  };
  const playing = (event: Event) => {
    if (!active && event.target instanceof HTMLMediaElement) suspend(event.target);
  };
  document.addEventListener("play", playing, true);
  const stop = subscribe((value) => {
    active = value;
    document.documentElement.dataset.motionPaused = String(!active);
    if (!active) {
      for (const media of document.querySelectorAll<HTMLMediaElement>("video, audio")) suspend(media);
    } else {
      for (const media of paused) {
        if (media.isConnected && !media.ended) {
          void media.play().catch((error: unknown) => console.debug("Could not resume media playback", error));
        }
      }
      paused.clear();
    }
  });
  return () => {
    stop();
    paused.clear();
    document.removeEventListener("play", playing, true);
    delete document.documentElement.dataset.motionPaused;
  };
}
