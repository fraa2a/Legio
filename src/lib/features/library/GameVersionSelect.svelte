<script lang="ts">
  import Icon from "../../components/ui/Icon.svelte";
  import SteamArtwork from "./SteamArtwork.svelte";

  interface VersionOption {
    id: string;
    name: string;
    subtitle: string;
    isSteam?: boolean;
  }

  let {
    steamAppId,
    options,
    selectedId,
    onSelect,
  }: {
    steamAppId: number | null;
    options: VersionOption[];
    selectedId: string;
    onSelect: (id: string) => void;
  } = $props();

  let open = $state(false);
  let root: HTMLDivElement;
  const selected = $derived(options.find((option) => option.id === selectedId) ?? options[0]);

  function outside(event: PointerEvent): void {
    if (root && !root.contains(event.target as Node)) open = false;
  }

  function choose(id: string): void {
    onSelect(id);
    open = false;
  }

  function handleKeydown(event: KeyboardEvent): void {
    if (event.key !== "Escape" || !open) return;
    event.preventDefault();
    open = false;
  }
</script>

<svelte:window onpointerdown={outside} />

<div bind:this={root} class="relative z-20 h-[60px] w-[260px] shrink-0">
  <button
    type="button"
    aria-label={`Seleziona versione: ${selected.name}, ${selected.subtitle}`}
    aria-expanded={open}
    onkeydown={handleKeydown}
    class="flex size-full items-center gap-2 border border-white/10 bg-zinc-950/60 px-3 text-left text-zinc-100 light:border-zinc-900/10 light:bg-zinc-100/70 light:text-zinc-900 {open ? 'rounded-t-xl border-b-0' : 'rounded-xl hover:bg-zinc-950/75 light:hover:bg-zinc-200'}"
    onclick={() => (open = !open)}
  >
    {#if steamAppId !== null}
      <SteamArtwork {steamAppId} asset="logo" caption={false} alt="" class="size-10 shrink-0 object-contain">
        {#snippet placeholder()}<span class="flex size-10 shrink-0 items-center justify-center rounded-md bg-zinc-700 text-sm font-semibold">{selected.name.charAt(0).toUpperCase()}</span>{/snippet}
      </SteamArtwork>
    {:else}
      <span class="flex size-10 shrink-0 items-center justify-center rounded-md bg-zinc-700 text-sm font-semibold">{selected.name.charAt(0).toUpperCase()}</span>
    {/if}
    <span class="flex min-w-0 flex-1 flex-col gap-0.5">
      <span class="flex min-w-0 items-center gap-1.5">
        <span class="truncate text-lg leading-6 font-semibold">{selected.name}</span>
        {#if selected.isSteam}<span class="shrink-0 rounded bg-sky-500/20 px-1.5 py-0.5 text-[10px] font-bold leading-none text-sky-300 light:text-sky-700">STEAM</span>{/if}
      </span>
      <span class="truncate text-xs leading-4 text-zinc-400 light:text-zinc-600">{selected.subtitle}</span>
    </span>
    <Icon name="chevron-down" size="h-4 w-4 shrink-0 text-zinc-400 light:text-zinc-600" />
  </button>
  {#if open}
    <div class="absolute left-0 top-full max-h-48 w-full overflow-y-auto rounded-b-xl border border-t-0 border-white/10 bg-zinc-950/60 p-1 light:border-zinc-900/10 light:bg-zinc-100/70">
      {#each options as option (option.id)}
        <button
          type="button"
          aria-current={option.id === selectedId ? "true" : undefined}
          onkeydown={handleKeydown}
          class="flex w-full items-center gap-2 rounded-lg px-2 py-2 text-left hover:bg-white/10 focus-visible:bg-white/10 light:hover:bg-zinc-100 light:focus-visible:bg-zinc-100"
          onclick={() => choose(option.id)}
        >
          {#if steamAppId !== null}
            <SteamArtwork {steamAppId} asset="logo" caption={false} alt="" class="size-9 shrink-0 object-contain">
              {#snippet placeholder()}<span class="flex size-9 shrink-0 items-center justify-center rounded-md bg-zinc-700 text-sm font-semibold">{option.name.charAt(0).toUpperCase()}</span>{/snippet}
            </SteamArtwork>
          {:else}
            <span class="flex size-9 shrink-0 items-center justify-center rounded-md bg-zinc-700 text-sm font-semibold">{option.name.charAt(0).toUpperCase()}</span>
          {/if}
          <span class="flex min-w-0 flex-col">
            <span class="flex min-w-0 items-center gap-1.5">
              <span class="truncate text-sm font-semibold text-zinc-100 light:text-zinc-900">{option.name}</span>
              {#if option.isSteam}<span class="shrink-0 rounded bg-sky-500/20 px-1.5 py-0.5 text-[10px] font-bold leading-none text-sky-300 light:text-sky-700">STEAM</span>{/if}
            </span>
            <span class="truncate text-xs text-zinc-400 light:text-zinc-600">{option.subtitle}</span>
          </span>
        </button>
      {/each}
    </div>
  {/if}
</div>
