<script lang="ts">
  import { onDestroy } from "svelte";
  import { language, t } from "../../i18n";
  import type { SteamMovie } from "../../services/steam-metadata";
  import { loadSteamTrailer } from "../../services/steam-trailer";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import Icon from "../../components/ui/Icon.svelte";
  import Spinner from "../../components/ui/Spinner.svelte";
  let { movie }: { movie: SteamMovie } = $props();
  let video: HTMLVideoElement | undefined = $state();
  let player: HTMLDivElement | undefined = $state();
  let error = $state<string | null>(null);
  let actionError = $state<string | null>(null);
  let playing = $state(false);
  let starting = $state(false);
  let buffering = $state(false);
  let time = $state(0);
  let duration = $state(0);
  let buffered = $state(0);
  let volume = $state(1);
  let muted = $state(false);
  let fullscreen = $state(false);
  let controlsVisible = $state(true);
  let scrubbing = $state(false);
  let revision = $state(0);
  let resumeAt = 0;
  let alive = true;
  let hideTimer: ReturnType<typeof setTimeout> | undefined;
  const canSeek = $derived(Number.isFinite(duration) && duration > 0);
  const progress = $derived(canSeek ? Math.min(100, time / duration * 100) : 0);
  const bufferedProgress = $derived(canSeek ? Math.min(100, buffered / duration * 100) : 0);
  const showControls = $derived(controlsVisible || !playing || buffering || error !== null);
  const canFullscreen = typeof document.documentElement.requestFullscreen === "function";
  $effect(() => {
    void revision;
    if (!video) return;
    error = null;
    buffering = false;
    return loadSteamTrailer(video, movie, () => {
      error = t("Il trailer non è disponibile. Controlla la connessione e riprova.", $language);
      starting = false;
      buffering = false;
    });
  });
  onDestroy(() => { alive = false; clearTimeout(hideTimer); });

  function revealControls(): void {
    if (!alive) return;
    controlsVisible = true;
    clearTimeout(hideTimer);
    hideTimer = setTimeout(() => { if (playing && !scrubbing) controlsVisible = false; }, 2400);
  }
  function formatTime(value: number): string {
    const seconds = Math.floor(Number.isFinite(value) ? Math.max(0, value) : 0);
    return `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, "0")}`;
  }
  async function togglePlayback(): Promise<void> {
    if (!video) return;
    if (playing || starting) { video.pause(); starting = false; revealControls(); return; }
    if (error !== null) { retry(); return; }
    starting = true;
    try { await video.play(); }
    catch (cause) {
      if (alive && !(cause instanceof Error && cause.name === "AbortError")) error = t("Impossibile avviare il trailer. Riprova.", $language);
    } finally { if (alive) starting = false; }
  }
  function retry(): void {
    resumeAt = time;
    playing = false;
    starting = false;
    controlsVisible = true;
    revision++;
  }
  function seek(value: number): void {
    if (!video || !canSeek) return;
    time = Math.max(0, Math.min(duration, value));
    video.currentTime = time;
    revealControls();
  }
  function changeVolume(value: number): void {
    if (!video) return;
    volume = Math.max(0, Math.min(1, value));
    muted = volume === 0;
    video.volume = volume;
    video.muted = muted;
    revealControls();
  }
  function toggleMute(): void {
    if (!video) return;
    muted = !muted;
    video.muted = muted;
    if (!muted && volume === 0) changeVolume(1);
    revealControls();
  }
  async function toggleFullscreen(): Promise<void> {
    try {
      if (document.fullscreenElement === player) await document.exitFullscreen();
      else await player?.requestFullscreen();
    } catch { actionError = t("Impossibile attivare lo schermo intero.", $language); }
  }
  function handleKey(event: KeyboardEvent): void {
    if (event.altKey || event.ctrlKey || event.metaKey) return;
    const tag = (event.target as HTMLElement).tagName;
    if (tag === "INPUT" || tag === "BUTTON") return;
    const key = event.key.toLowerCase();
    if (![" ", "k", "m", "f", "arrowleft", "arrowright", "arrowup", "arrowdown"].includes(key)) return;
    event.preventDefault();
    revealControls();
    if (key === " " || key === "k") void togglePlayback();
    else if (key === "m") toggleMute();
    else if (key === "f" && canFullscreen) void toggleFullscreen();
    else if (key === "arrowleft") seek(time - 5);
    else if (key === "arrowright") seek(time + 5);
    else changeVolume(volume + (key === "arrowup" ? 0.05 : -0.05));
  }
</script>

<svelte:document onfullscreenchange={() => { fullscreen = document.fullscreenElement === player; }} onpointerup={() => { scrubbing = false; }} onpointercancel={() => { scrubbing = false; }} />
<div class="flex flex-col gap-2">
  <!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions (The media group supports playback shortcuts without focusing an individual control.) -->
  <div bind:this={player} class="trailer-player group relative isolate overflow-hidden rounded-2xl bg-black text-white" role="group" aria-label={movie.name || t("Trailer", $language)} tabindex="0" onkeydown={handleKey} onpointermove={revealControls}>
    <video bind:this={video} playsinline preload="metadata" poster={movie.thumbnail ?? undefined}
      aria-label={movie.name || t("Trailer", $language)} class="aspect-video w-full bg-black object-contain"
      onloadedmetadata={() => { duration = video?.duration ?? 0; if (video && resumeAt > 0 && Number.isFinite(duration)) { video.currentTime = Math.min(resumeAt, duration); resumeAt = 0; } }}
      ondurationchange={() => { duration = video?.duration ?? 0; }}
      ontimeupdate={() => { if (!scrubbing) time = video?.currentTime ?? 0; }}
      onprogress={() => { const ranges = video?.buffered; buffered = ranges?.length ? ranges.end(ranges.length - 1) : 0; }}
      onplay={() => { playing = true; revealControls(); }} onplaying={() => { buffering = false; }} onwaiting={() => { buffering = true; }}
      onseeked={() => { buffering = false; }} onpause={() => { playing = false; buffering = false; revealControls(); }} onended={() => { playing = false; controlsVisible = true; }}
      onvolumechange={() => { volume = video?.volume ?? 1; muted = video?.muted ?? false; }}></video>

    {#if error !== null}
      <div class="absolute inset-0 z-20 flex flex-col items-center justify-center gap-4 bg-black/85 px-6 text-center">
        <Icon name="warning" size="size-7 text-zinc-300" />
        <p class="max-w-md text-sm text-zinc-200" role="alert">{error}</p>
        <button type="button" class="flex items-center gap-2 rounded-full bg-white px-5 py-2 text-sm font-semibold text-black hover:bg-zinc-200" onclick={retry}><Icon name="reload" size="size-4" />{t("Riprova", $language)}</button>
      </div>
    {:else if buffering || starting}
      <div class="pointer-events-none absolute inset-0 grid place-items-center"><span class="rounded-full bg-black/65 p-4" role="status" aria-label={t("Caricamento del trailer...", $language)}><Spinner /></span></div>
    {:else}
      <button type="button" class="absolute inset-0 grid place-items-center" tabindex="-1" aria-hidden="true" onclick={() => void togglePlayback()}>{#if !playing}<span class="grid size-16 place-items-center rounded-full border border-white/20 bg-black/45 text-white transition-transform hover:scale-105 motion-reduce:transition-none"><Icon name="play" size="size-7" /></span>{/if}</button>
    {/if}

    <div class="trailer-controls pointer-events-none absolute inset-x-0 bottom-0 z-10 bg-gradient-to-t from-black/95 via-black/65 to-transparent px-4 pt-12 pb-3 transition-opacity duration-200 motion-reduce:transition-none sm:px-5 sm:pb-4 {showControls ? 'opacity-100' : 'opacity-0'}" role="group" aria-label={t("Controlli del trailer", $language)}>
      <input type="range" min="0" max={canSeek ? duration : 1} step="0.1" value={time} disabled={!canSeek} aria-label={t("Avanzamento del trailer", $language)} aria-valuetext={`${formatTime(time)} / ${formatTime(duration)}`}
        class="trailer-range trailer-seek pointer-events-auto w-full disabled:cursor-default disabled:opacity-40" style={`--played:${progress}%;--buffered:${bufferedProgress}%`}
        onpointerdown={() => { scrubbing = true; }} onchange={() => { scrubbing = false; }} oninput={event => seek(Number(event.currentTarget.value))} />
      <div class="pointer-events-auto flex items-center gap-1 sm:gap-2">
        <button type="button" class="trailer-button" aria-label={playing || starting ? t("Metti in pausa", $language) : t("Riproduci trailer", $language)} onclick={() => void togglePlayback()}><Icon name={playing || starting ? "pause" : "play"} /></button>
        <button type="button" class="trailer-button trailer-skip text-xs font-semibold" aria-label={t("Indietro di 10 secondi", $language)} disabled={!canSeek} onclick={() => seek(time - 10)}>-10</button>
        <button type="button" class="trailer-button trailer-skip text-xs font-semibold" aria-label={t("Avanti di 10 secondi", $language)} disabled={!canSeek} onclick={() => seek(time + 10)}>+10</button>
        <div class="group/volume ml-1 flex items-center gap-1">
          <button type="button" class="trailer-button" aria-label={muted ? t("Attiva audio", $language) : t("Disattiva audio", $language)} onclick={toggleMute}><Icon name={muted || volume === 0 ? "muted" : "volume"} /></button>
          <input type="range" min="0" max="1" step="0.05" value={muted ? 0 : volume} aria-label={t("Volume del trailer", $language)} class="trailer-range w-16 sm:w-20" style={`--played:${muted ? 0 : volume * 100}%;--buffered:0%`} oninput={event => changeVolume(Number(event.currentTarget.value))} />
        </div>
        <span class="ml-2 whitespace-nowrap text-xs tabular-nums text-zinc-200">{formatTime(time)} / {formatTime(duration)}</span>
        <span class="ml-auto hidden min-w-0 truncate px-3 text-xs text-white/60 lg:block">{movie.name}</span>
        <button type="button" class="trailer-button ml-auto lg:ml-0" disabled={!canFullscreen} aria-label={fullscreen ? t("Esci da schermo intero", $language) : t("Schermo intero", $language)} onclick={() => void toggleFullscreen()}><Icon name={fullscreen ? "restore" : "maximize"} /></button>
      </div>
    </div>
  </div>
  {#if actionError}<ErrorBanner message={actionError} />{/if}
</div>
<style>
  @media (max-width:639px) { .trailer-button.trailer-skip { display:none; } }
  .trailer-controls:focus-within { opacity: 1; }
  .trailer-button { display:grid;place-items:center;width:2.5rem;height:2.5rem;flex-shrink:0;border-radius:50%;color:white;cursor:pointer; }
  .trailer-button:hover { background:rgb(255 255 255 / .12); }
  .trailer-button:disabled { opacity:.35;cursor:default; }
  .trailer-range { appearance:none;height:1.25rem;background:transparent;cursor:pointer; }
  .trailer-range::-webkit-slider-runnable-track { height:3px;border-radius:999px;background:linear-gradient(to right,var(--legio-accent,#fff) var(--played),transparent var(--played)),linear-gradient(to right,rgb(255 255 255 / .5) var(--buffered),rgb(255 255 255 / .22) var(--buffered)); }
  .trailer-range::-moz-range-track { height:3px;border-radius:999px;background:linear-gradient(to right,var(--legio-accent,#fff) var(--played),transparent var(--played)),linear-gradient(to right,rgb(255 255 255 / .5) var(--buffered),rgb(255 255 255 / .22) var(--buffered)); }
  .trailer-range::-webkit-slider-thumb { appearance:none;width:11px;height:11px;margin-top:-4px;border-radius:50%;background:white; }
  .trailer-range::-moz-range-thumb { width:11px;height:11px;border:0;border-radius:50%;background:white; }
  .trailer-player:fullscreen { display:flex;align-items:center;justify-content:center;width:100%;height:100%;border-radius:0; }
  .trailer-player:fullscreen video { width:100%;height:100%; }
</style>
