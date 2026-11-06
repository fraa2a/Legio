import type { SteamMovie } from "./steam-metadata";
import { toMessage } from "../utils/errors";

export function loadSteamTrailer(video: HTMLVideoElement, movie: SteamMovie, onError: (message: string) => void): () => void {
  let disposed = false;
  let failed = false;
  let stream: { destroy: () => void } | undefined;
  function fail(message: string): void {
    if (disposed || failed) return;
    failed = true;
    stream?.destroy();
    stream = undefined;
    onError(message);
  }
  const nativeError = () => {
    if (stream === undefined) fail(video.error?.message || "Media playback failed.");
  };
  video.addEventListener("error", nativeError);
  async function load(): Promise<void> {
    if (movie.hls !== null) {
      const { default: Hls } = await import("hls.js");
      if (disposed) return;
      if (Hls.isSupported()) {
        const hls = new Hls({ enableWorker: false, maxBufferLength: 20, maxMaxBufferLength: 30 });
        stream = hls;
        let recoveries = 0;
        hls.on(Hls.Events.ERROR, (_event, data) => {
          if (!data.fatal || disposed || failed) return;
          if (recoveries < 2 && data.type === Hls.ErrorTypes.NETWORK_ERROR) {
            recoveries++;
            hls.startLoad(Number.isFinite(video.currentTime) ? video.currentTime : 0);
          } else if (recoveries < 2 && data.type === Hls.ErrorTypes.MEDIA_ERROR) {
            recoveries++;
            hls.recoverMediaError();
          } else fail(data.details);
        });
        hls.loadSource(movie.hls);
        hls.attachMedia(video);
        return;
      }
      if (video.canPlayType("application/vnd.apple.mpegurl")) {
        video.src = movie.hls;
        return;
      }
    }
    const file = movie.mp4 ?? movie.webm;
    if (file !== null) video.src = file;
    else fail("HLS playback is not supported by this browser.");
  }
  void load().catch(error => fail(toMessage(error)));
  return () => {
    disposed = true;
    video.removeEventListener("error", nativeError);
    stream?.destroy();
    video.pause();
    video.removeAttribute("src");
    video.load();
  };
}
