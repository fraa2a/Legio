<script lang="ts">
  import type { SteamDetails } from "../../services/steam-details";
  import ArtworkViewer from "./ArtworkViewer.svelte";
  import SteamArtwork from "./SteamArtwork.svelte";

  let {
    steamAppId,
    name,
    version = null,
    screenshots,
  }: {
    steamAppId: number;
    name: string;
    version?: number | null;
    screenshots: SteamDetails["assets"]["screenshots"];
  } = $props();

  const count = $derived(screenshots.length);
  let selected = $state(0);
  const active = $derived(Math.max(0, Math.min(selected, count - 1)));
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
  <button
    type="button"
    class="relative w-full cursor-zoom-in overflow-hidden rounded-2xl bg-white/5 ring-1 ring-white/10 light:bg-zinc-200"
    aria-label="Ingrandisci schermata {active + 1} di {count}"
    onclick={() => (open = true)}
  >
    <SteamArtwork
      {steamAppId}
      asset="screenshot"
      index={active}
      {version}
      full
      alt="Schermata {active + 1} di {name}"
      caption={false}
      class="aspect-video w-full object-cover"
    />
    {#if count > 1}
      <span
        class="absolute right-3 bottom-3 rounded-full bg-black/70 px-2.5 py-1 text-xs tabular-nums text-zinc-100 backdrop-blur-sm"
      >
        {active + 1} / {count}
      </span>
    {/if}
  </button>

  <div bind:this={strip} class="overflow-x-auto px-1.5 pt-1.5 pb-4">
    <div class="flex gap-2.5">
      {#each screenshots as screenshot, position (screenshot.thumbnail ?? screenshot.full)}
        <button
          type="button"
          data-index={position}
          class="aspect-video min-w-28 flex-1 overflow-hidden rounded-xl bg-white/5 transition duration-200 light:bg-zinc-200 {position
            === active
            ? 'opacity-100 ring-2 ring-white light:ring-zinc-900'
            : 'opacity-60 ring-1 ring-white/10 hover:opacity-100'}"
          aria-label="Mostra schermata {position + 1}"
          aria-current={position === active}
          onclick={() => select(position)}
        >
          <SteamArtwork
            {steamAppId}
            asset="screenshot"
            index={position}
            {version}
            alt=""
            caption={false}
            class="size-full object-cover"
          />
        </button>
      {/each}
    </div>
  </div>
</div>

{#if open}
  <ArtworkViewer
    {steamAppId}
    asset="screenshot"
    index={active}
    {count}
    {version}
    alt="Schermata {active + 1} di {name}"
    onSelect={select}
    onClose={() => (open = false)}
  />
{/if}
