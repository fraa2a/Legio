<script lang="ts">
  import { t, language } from "../../i18n";
  import type { SteamDetails } from "../../services/steam-details";
  import Icon from "../../components/ui/Icon.svelte";
  import TrailerPlayer from "./TrailerPlayer.svelte";
  import ArtworkViewer from "./ArtworkViewer.svelte";
  import SteamArtwork from "./SteamArtwork.svelte";

  let {
    steamAppId,
    name,
    screenshots,
    movies = [],
  }: {
    steamAppId: number;
    name: string;
    screenshots: SteamDetails["assets"]["screenshots"];
    movies?: NonNullable<SteamDetails["assets"]["movies"]>;
  } = $props();

  const count = $derived(screenshots.length);
  const mediaCount = $derived(movies.length + count);
  let selected = $state(0);
  const active = $derived(Math.max(0, Math.min(selected, mediaCount - 1)));
  const activeMovie = $derived(active < movies.length ? movies[active] : null);
  const screenshotIndex = $derived(active - movies.length);
  let open = $state(false);

  let strip: HTMLElement | undefined = $state();

  function select(index: number): void {
    selected = index;
    strip
      ?.querySelector<HTMLElement>(`[data-index="${index}"]`)
      ?.scrollIntoView({ block: "nearest", inline: "center" });
  }
</script>

<div class="flex flex-col gap-3">
  {#if activeMovie !== null}
    {#key activeMovie.id}<TrailerPlayer movie={activeMovie} />{/key}
  {:else}
  <button
    type="button"
    class="relative w-full cursor-zoom-in overflow-hidden rounded-2xl bg-white/5 ring-1 ring-legio-line-subtle light:bg-zinc-200"
    aria-label={t("Ingrandisci schermata {0} di {1}", $language, [screenshotIndex + 1, count])}
    onclick={() => (open = true)}
  >
    <SteamArtwork
      {steamAppId}
      asset="screenshot"
      index={screenshotIndex}
      screenshotIdentity={screenshots[screenshotIndex]?.full ?? screenshots[screenshotIndex]?.thumbnail ?? undefined}

      full
      alt={t("Schermata {0} di {1}", $language, [screenshotIndex + 1, name])}
      caption={false}
      class="aspect-video w-full object-cover"
    />
    {#if count > 1}
      <span
        class="absolute right-3 bottom-3 rounded-full bg-black/70 px-2.5 py-1 text-xs tabular-nums text-zinc-100 backdrop-blur-sm"
      >
        {screenshotIndex + 1} / {count}
      </span>
    {/if}
  </button>
  {/if}

  <div bind:this={strip} class="overflow-x-auto px-1.5 pt-1.5 pb-4">
    <div class="flex gap-2.5">
      {#each movies as movie, position (movie.id)}
        <button type="button" data-index={position} class="relative aspect-video min-w-28 flex-1 overflow-hidden rounded-xl bg-zinc-800 transition-opacity {position === active ? 'ring-2 ring-legio-fg' : 'opacity-60 hover:opacity-100'}"
          aria-label={t("Mostra trailer: {0}", $language, [movie.name || t("Trailer", $language)])} aria-current={position === active} onclick={() => select(position)}>
          {#if movie.thumbnail}<img src={movie.thumbnail} alt="" loading="lazy" referrerpolicy="no-referrer" class="size-full object-cover" />{/if}
          <span class="absolute inset-0 grid place-items-center bg-black/20"><span class="grid size-8 place-items-center rounded-full bg-black/70 text-white"><Icon name="play" size="size-4" /></span></span>
        </button>
      {/each}
      {#each screenshots as screenshot, position (screenshot.thumbnail ?? screenshot.full)}
        <button
          type="button"
          data-index={movies.length + position}
          class="aspect-video min-w-28 flex-1 overflow-hidden rounded-xl bg-white/5 transition duration-200 light:bg-zinc-200 {movies.length + position
            === active
            ? 'opacity-100 ring-2 ring-legio-fg'
            : 'opacity-60 ring-1 ring-legio-line-subtle hover:opacity-100'}"
          aria-label={t("Mostra schermata {0}", $language, [position + 1])}
          aria-current={movies.length + position === active}
          onclick={() => select(movies.length + position)}
        >
          <SteamArtwork
            {steamAppId}
            asset="screenshot"
            index={position}
            screenshotIdentity={screenshot.thumbnail ?? screenshot.full ?? undefined}

            alt=""
            caption={false}
            class="size-full object-cover"
          />
        </button>
      {/each}
    </div>
  </div>
</div>

{#if open && activeMovie === null}
  <ArtworkViewer
    {steamAppId}
    asset="screenshot"
    index={screenshotIndex}
    screenshotIdentity={screenshots[screenshotIndex]?.full ?? screenshots[screenshotIndex]?.thumbnail ?? undefined}
    {count}

    alt={t("Schermata {0} di {1}", $language, [screenshotIndex + 1, name])}
    onSelect={(index) => select(movies.length + index)}
    onClose={() => (open = false)}
  />
{/if}
