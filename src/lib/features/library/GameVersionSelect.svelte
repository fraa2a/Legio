<script lang="ts">
  import { t, language } from "../../i18n";
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
    onAdd,
  }: {
    steamAppId: number | null;
    options: VersionOption[];
    selectedId: string;
    onSelect: (id: string) => void;
    onAdd?: () => void;
  } = $props();

  let open = $state(false);
  let root: HTMLDivElement;
  const selected = $derived(options.find((option) => option.id === selectedId) ?? options[0]);
  const alternatives = $derived(options.filter((option) => option.id !== selectedId));
  const hasChoices = $derived(alternatives.length > 0 || onAdd !== undefined);

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
  <div class="absolute bottom-0 left-0 flex w-full flex-col-reverse overflow-hidden rounded-xl border border-white/10 bg-zinc-950/60 backdrop-blur-md light:border-zinc-900/10 light:bg-zinc-100/70 {open ? 'shadow-xl' : 'hover:bg-zinc-950/75 light:hover:bg-zinc-200'}">
    <button
      type="button"
      aria-label={t("Seleziona versione: {0}, {1}", $language, [selected.name, selected.subtitle])}
      aria-expanded={open}
      disabled={!hasChoices}
      onkeydown={handleKeydown}
      class="flex h-[58px] w-full items-center gap-2 bg-transparent px-3 text-left text-zinc-100 light:text-zinc-900"
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
      {#if hasChoices}<span class="transition-transform duration-150 {open ? 'rotate-180' : ''}"><Icon name="chevron-down" size="h-4 w-4 shrink-0 text-zinc-400 light:text-zinc-600" /></span>{/if}
    </button>
    {#if open}
      <div class="max-h-[188px] overflow-y-auto overscroll-contain p-1">
        {#each alternatives as option (option.id)}
          <button
            type="button"
            aria-current={option.id === selectedId ? "true" : undefined}
            onkeydown={handleKeydown}
            class="flex h-[60px] w-full items-center gap-2 rounded-lg px-2 py-2 text-left hover:bg-white/10 focus-visible:bg-white/10 light:hover:bg-zinc-900/10 light:focus-visible:bg-zinc-900/10"
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
        {#if onAdd}
          <button type="button" class="flex h-[60px] w-full items-center gap-2 rounded-lg px-2 py-2 text-left text-sm text-zinc-200 hover:bg-white/10 focus-visible:bg-white/10 light:text-zinc-800 light:hover:bg-zinc-900/10" onclick={() => { open = false; onAdd(); }}>
            <Icon name="plus" size="size-5" />{t("Aggiungi un'altra versione", $language)}
          </button>
        {/if}
      </div>
    {/if}
  </div>
</div>
