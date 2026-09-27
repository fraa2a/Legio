<script lang="ts">
  import type { SteamAssetKind } from "../../services/steam-details";
  import { fade } from "svelte/transition";
  import Button from "../../components/ui/Button.svelte";
  import Icon from "../../components/ui/Icon.svelte";
  import { fadeDuration } from "../../utils/motion";
  import SteamArtwork from "./SteamArtwork.svelte";

  let {
    steamAppId,
    asset,
    index = null,
    count = 1,
    version = null,
    alt = "",
    onSelect,
    onClose,
  }: {
    steamAppId: number;
    asset: SteamAssetKind;
    index?: number | null;
    count?: number;
    version?: number | null;
    alt?: string;
    onSelect?: (index: number) => void;
    onClose: () => void;
  } = $props();

  const position = $derived(index === null || count < 2 ? null : `${index + 1} / ${count}`);
  const slots = $derived(Array.from({ length: count }, (_, slot) => slot));

  let overlay: HTMLElement | undefined = $state();

  $effect(() => {
    overlay?.focus();
  });

  function previous(): void {
    if (onSelect === undefined || count < 2) return;
    onSelect(((index ?? 0) - 1 + count) % count);
  }

  function next(): void {
    if (onSelect === undefined || count < 2) return;
    onSelect(((index ?? 0) + 1) % count);
  }

  function handleKeydown(event: KeyboardEvent): void {
    if (event.key === "Escape") {
      onClose();
      return;
    }
    if (event.key === "ArrowLeft") {
      previous();
      return;
    }
    if (event.key === "ArrowRight") next();
  }
</script>

<svelte:window onkeydown={handleKeydown} />

<div
  bind:this={overlay}
  role="dialog"
  aria-modal="true"
  aria-label="Immagine a schermo intero"
  tabindex="-1"
  class="fixed inset-0 z-50 flex flex-col bg-black/90 backdrop-blur-sm focus:outline-none light:bg-zinc-950/95"
>
  <div class="flex shrink-0 items-center justify-between gap-3 p-4">
    <span class="text-sm text-zinc-300 tabular-nums">{position ?? ""}</span>
    <Button label="Chiudi" variant="secondary" circle onClick={onClose} class="size-10">
      <Icon name="close" size="h-5 w-5" />
    </Button>
  </div>

  <div class="relative min-h-0 flex-1">
    <button
      type="button"
      class="absolute inset-0 cursor-zoom-out"
      aria-label="Chiudi"
      aria-hidden="true"
      tabindex="-1"
      onclick={onClose}
    ></button>
    <div class="pointer-events-none absolute inset-0">
      {#key `${asset}-${index}`}
        <div
          class="absolute inset-0 flex items-center justify-center p-4"
          transition:fade={{ duration: fadeDuration }}
        >
          <SteamArtwork
            {steamAppId}
            {asset}
            {index}
            {version}
            full
            {alt}
            caption={false}
            class="h-full max-h-full w-auto max-w-full rounded-2xl object-contain"
          />
        </div>
      {/key}
    </div>
    {#if count > 1}
      <Button
        label="Immagine precedente"
        variant="secondary"
        circle
        onClick={previous}
        class="absolute left-4 top-1/2 size-10 -translate-y-1/2"
      >
        <Icon name="previous" size="h-6 w-6" />
      </Button>
      <Button
        label="Immagine successiva"
        variant="secondary"
        circle
        onClick={next}
        class="absolute right-4 top-1/2 size-10 -translate-y-1/2"
      >
        <Icon name="next" size="h-6 w-6" />
      </Button>
    {/if}
  </div>

  {#if count > 1}
    <div class="flex shrink-0 items-center justify-center gap-2 overflow-x-auto p-4">
      {#each slots as slot (slot)}
        <button
          type="button"
          class="h-16 w-28 shrink-0 overflow-hidden rounded-lg transition-opacity {slot === index
            ? 'opacity-100 ring-2 ring-white'
            : 'opacity-50 hover:opacity-100'}"
          aria-label="Vai all'immagine {slot + 1}"
          aria-current={slot === index}
          onclick={() => onSelect?.(slot)}
        >
          <SteamArtwork
            {steamAppId}
            {asset}
            index={slot}
            {version}
            alt=""
            caption={false}
            class="size-full object-cover"
          />
        </button>
      {/each}
    </div>
  {/if}
</div>
