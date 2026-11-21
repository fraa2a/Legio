export interface SteamVideoSurface {
  element: HTMLElement;
  // Far from the viewport the video drops its source, so it holds no decoder or buffered frames.
  setNear: (near: boolean) => void;
  destroy: () => void;
}

export function createSteamVideoSurface(video: HTMLVideoElement): SteamVideoSurface {
  const source = video.getAttribute("src");
  const sources = [...video.querySelectorAll("source")];
  let loaded = true;
  const canvas = document.createElement("canvas");
  const context = canvas.getContext("2d");

  function unload(): void {
    video.pause();
    video.removeAttribute("src");
    video.replaceChildren();
    video.load();
  }

  function reload(): void {
    if (source !== null) video.src = source;
    video.append(...sources);
    video.load();
  }

  if (context === null) {
    return {
      element: video,
      setNear(near) {
        if (near === loaded) return;
        loaded = near;
        if (near) reload();
        else unload();
      },
      destroy: () => video.pause(),
    };
  }

  const element = document.createElement("span");
  element.className = "relative my-3 block w-fit max-w-full overflow-hidden rounded-control";
  video.className = "block h-auto max-w-full";
  canvas.className = "pointer-events-none absolute inset-0 size-full";
  canvas.setAttribute("aria-hidden", "true");
  canvas.hidden = true;
  element.append(video, canvas);

  const nativeFrames = typeof video.requestVideoFrameCallback === "function";
  let frame: number | undefined;
  let destroyed = false;

  function paint(): void {
    if (video.seeking || video.readyState < 2 || video.videoWidth === 0 || video.videoHeight === 0) return;
    if (canvas.width !== video.videoWidth) canvas.width = video.videoWidth;
    if (canvas.height !== video.videoHeight) canvas.height = video.videoHeight;
    context!.drawImage(video, 0, 0, canvas.width, canvas.height);
    // Retain the decoded frame while the native video surface resets at a loop.
    canvas.hidden = false;
  }

  function schedule(): void {
    if (destroyed || !loaded || frame !== undefined) return;
    const next = () => {
      frame = undefined;
      if (destroyed) return;
      paint();
      if (nativeFrames || !video.paused) schedule();
    };
    frame = nativeFrames ? video.requestVideoFrameCallback(next) : requestAnimationFrame(next);
  }

  function cancel(): void {
    if (frame === undefined) return;
    if (nativeFrames) video.cancelVideoFrameCallback(frame);
    else cancelAnimationFrame(frame);
    frame = undefined;
  }

  if (nativeFrames) schedule();
  else {
    video.addEventListener("loadeddata", paint);
    video.addEventListener("playing", schedule);
    video.addEventListener("pause", cancel);
  }

  return {
    element,
    setNear(near) {
      if (destroyed || near === loaded) return;
      loaded = near;
      if (near) {
        reload();
        if (nativeFrames) schedule();
        return;
      }
      cancel();
      unload();
      // Drops the retained frame's backing store; the poster shows until the video returns.
      canvas.hidden = true;
      canvas.width = 0;
      canvas.height = 0;
    },
    destroy() {
      if (destroyed) return;
      destroyed = true;
      cancel();
      video.removeEventListener("loadeddata", paint);
      video.removeEventListener("playing", schedule);
      video.removeEventListener("pause", cancel);
      video.pause();
    },
  };
}
