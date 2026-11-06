export function createSteamVideoSurface(video: HTMLVideoElement): { element: HTMLElement; destroy: () => void } {
  const canvas = document.createElement("canvas");
  const context = canvas.getContext("2d");
  if (context === null) return { element: video, destroy: () => video.pause() };

  const element = document.createElement("span");
  element.className = "relative my-3 block w-fit max-w-full overflow-hidden rounded-lg";
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
    if (destroyed || frame !== undefined) return;
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

  return { element, destroy() {
    if (destroyed) return;
    destroyed = true;
    cancel();
    video.removeEventListener("loadeddata", paint);
    video.removeEventListener("playing", schedule);
    video.removeEventListener("pause", cancel);
    video.pause();
  } };
}
