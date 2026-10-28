<script lang="ts" module>
  export interface ReleaseOption {
    id: string;
    name: string;
    subtitle: string;
    sourceName?: string;
    sizeBytes?: number | null;
    availability?: "verified" | "unverified";
    isSteam?: boolean;
  }
</script>

<script lang="ts">
  import { tick } from "svelte";
  import { t, language } from "../../i18n";
  import { formatBytes } from "../../utils/format";
  import { placePopover } from "../../utils/popover-placement";
  import Badge from "../../components/ui/Badge.svelte";
  import Icon from "../../components/ui/Icon.svelte";
  import { availabilityMeta } from "../store/source-status";
  import SteamArtwork from "./SteamArtwork.svelte";

  let {
    steamAppId,
    options,
    selectedId,
    onSelect,
    onAdd,
    links = [],
    link = $bindable("auto"),
    disabled = false,
    linksDisabled = false,
    class: className = "",
  }: {
    steamAppId: number | null;
    options: ReleaseOption[];
    selectedId: string;
    onSelect: (id: string) => void;
    onAdd?: () => void;
    links?: string[];
    link?: string;
    disabled?: boolean;
    linksDisabled?: boolean;
    class?: string;
  } = $props();

  const panelId = $props.id();
  const panelWidth = 352;
  const panelMargin = 8;
  const panelMaxHeight = 420;

  let trigger: HTMLButtonElement;
  let panel = $state<HTMLDivElement>();
  let open = $state(false);
  let placement = $state({ top: 0, left: 0, width: panelWidth, maxHeight: panelMaxHeight });

  const selected = $derived(options.find((option) => option.id === selectedId) ?? options[0]);
  const showLinks = $derived(links.length > 1);
  const linkOptions = $derived([
    { value: "auto", label: t("Automatico", $language) },
    ...links.map((uri) => ({ value: uri, label: linkLabel(uri) })),
  ]);
  const chosenLink = $derived(link === "auto" ? null : (linkOptions.find((option) => option.value === link)?.label ?? null));
  const hasChoices = $derived(options.length > 1 || onAdd !== undefined || showLinks);

  function linkLabel(uri: string): string {
    if (uri.startsWith("magnet:")) return "Torrent";
    try {
      return new URL(uri).hostname;
    } catch {
      return uri;
    }
  }

  function details(option: ReleaseOption): string {
    const size = option.sizeBytes === undefined ? null : (option.sizeBytes === null ? t("Dimensione sconosciuta", $language) : formatBytes(option.sizeBytes, $language));
    return size === null ? option.subtitle : `${option.subtitle} · ${size}`;
  }

  $effect(() => {
    if (link !== "auto" && !links.includes(link)) link = "auto";
  });

  $effect(() => {
    if (disabled || !hasChoices) open = false;
  });

  $effect(() => {
    if (!open) return;
    const closeOnScroll = (event: Event) => {
      if (!(event.target instanceof Node && panel?.contains(event.target))) open = false;
    };
    const closeOnResize = () => (open = false);
    window.addEventListener("scroll", closeOnScroll, true);
    window.addEventListener("resize", closeOnResize);
    return () => {
      window.removeEventListener("scroll", closeOnScroll, true);
      window.removeEventListener("resize", closeOnResize);
    };
  });

  async function showPanel(): Promise<void> {
    if (disabled || !hasChoices) return;
    open = true;
    await tick();
    if (!open || !panel) return;
    const triggerRect = trigger.getBoundingClientRect();
    const width = Math.min(Math.max(triggerRect.width, panelWidth), window.innerWidth - 2 * panelMargin);
    const left = Math.max(panelMargin, Math.min(triggerRect.left, window.innerWidth - width - panelMargin));
    placement = { top: 0, left, width, maxHeight: panelMaxHeight };
    await tick();
    if (!open || !panel?.isConnected) return;
    panel.showPopover();
    const next = placePopover(triggerRect, panel.scrollHeight, window.innerHeight, panelMaxHeight);
    placement = { top: next.top, left, width, maxHeight: next.maxHeight };
    await tick();
    if (!open || !panel?.isConnected) return;
    (panel.querySelector<HTMLElement>('[aria-selected="true"]') ?? panel.querySelector<HTMLElement>("button"))?.focus({ preventScroll: true });
  }

  function close(): void {
    open = false;
    trigger.focus();
  }

  function choose(id: string): void {
    onSelect(id);
    close();
  }

  function handleKeydown(event: KeyboardEvent): void {
    if (event.key === "Escape" && open) {
      event.preventDefault();
      close();
      return;
    }
    if (!["ArrowDown", "ArrowUp", "Home", "End"].includes(event.key)) return;
    if (!open) {
      event.preventDefault();
      void showPanel();
      return;
    }
    const group = event.target instanceof Element ? event.target.closest<HTMLElement>("[data-group]") : null;
    if (group === null) return;
    event.preventDefault();
    const items = [...group.querySelectorAll<HTMLButtonElement>("button")];
    const current = items.indexOf(document.activeElement as HTMLButtonElement);
    const next = event.key === "Home" ? 0 : event.key === "End" ? items.length - 1
      : event.key === "ArrowDown" ? (current + 1) % items.length
      : (current - 1 + items.length) % items.length;
    items[next]?.focus();
  }
</script>

{#snippet icon(name: string, size: string)}
  {#if steamAppId !== null}
    <SteamArtwork {steamAppId} asset="client_icon" caption={false} alt="" class="{size} shrink-0 rounded-md object-contain">
      {#snippet placeholder()}{@render monogram(name, size)}{/snippet}
    </SteamArtwork>
  {:else}
    {@render monogram(name, size)}
  {/if}
{/snippet}

{#snippet monogram(name: string, size: string)}
  <span class="flex {size} shrink-0 items-center justify-center rounded-md bg-zinc-700/60 text-sm font-semibold text-zinc-100 light:bg-zinc-200 light:text-zinc-800" aria-hidden="true">{name.trim().charAt(0).toUpperCase() || "?"}</span>
{/snippet}

{#snippet badges(option: ReleaseOption)}
  {#if option.sourceName}<span class="max-w-32 min-w-0 shrink" title={option.sourceName}><Badge title={option.sourceName} /></span>{/if}
  {#if option.isSteam}<Badge tone="info" title="STEAM" />{/if}
  {#if option.availability}<Badge tone={availabilityMeta[option.availability].tone} title={t(availabilityMeta[option.availability].label, $language)} />{/if}
{/snippet}

<svelte:window onfocusin={(event) => {
  if (open && !panel?.contains(event.target as Node) && event.target !== trigger) open = false;
}} />

<div class="relative {className}">
  <button
    bind:this={trigger}
    type="button"
    aria-label={t("Seleziona versione: {0}, {1}", $language, [selected.name, selected.subtitle])}
    aria-haspopup="listbox"
    aria-expanded={open}
    aria-controls={open ? panelId : undefined}
    disabled={disabled || !hasChoices}
    onkeydown={handleKeydown}
    onclick={() => (open ? close() : void showPanel())}
    class="flex h-15 w-full items-center gap-3 rounded-xl border border-white/10 bg-white/5 px-3 text-left text-zinc-100 transition-colors enabled:hover:border-white/25 enabled:hover:bg-white/10 disabled:cursor-default light:border-zinc-900/10 light:bg-white light:text-zinc-900 light:enabled:hover:border-zinc-900/25 light:enabled:hover:bg-zinc-50"
  >
    {@render icon(selected.name, "size-10")}
    <span class="flex min-w-0 flex-1 flex-col gap-0.5">
      <span class="flex min-w-0 items-center gap-1.5">
        <span class="truncate text-base leading-5 font-semibold">{selected.name}</span>
        {@render badges(selected)}
      </span>
      <span class="truncate text-xs leading-4 text-zinc-400 light:text-zinc-600">{details(selected)}{chosenLink === null ? "" : ` · ${chosenLink}`}</span>
    </span>
    {#if hasChoices}<span class="shrink-0 text-zinc-400 transition-transform duration-150 light:text-zinc-600 {open ? 'rotate-180' : ''}"><Icon name="chevron-down" size="size-4" /></span>{/if}
  </button>

  {#if open}
    <div
      bind:this={panel}
      id={panelId}
      popover="auto"
      role="presentation"
      onkeydown={handleKeydown}
      ontoggle={(event) => { if (event.newState === "closed") open = false; }}
      style:inset="auto"
      style:top="{placement.top}px"
      style:left="{placement.left}px"
      style:width="{placement.width}px"
      style:max-height="{placement.maxHeight}px"
      class="fixed z-50 m-0 flex flex-col gap-2 overflow-auto rounded-2xl border border-white/10 bg-zinc-900/95 p-2 text-zinc-100 shadow-xl shadow-black/30 backdrop-blur-xl light:border-zinc-900/10 light:bg-zinc-50/95 light:text-zinc-900 light:shadow-zinc-900/15"
    >
      <div data-group>
        <p id="{panelId}-releases" class="px-2 pt-1 pb-1.5 text-xs font-medium text-zinc-400 light:text-zinc-600">{t("Versione", $language)}</p>
        <div role="listbox" aria-labelledby="{panelId}-releases" class="flex flex-col gap-0.5">
          {#each options as option (option.id)}
            <button
              type="button"
              role="option"
              aria-selected={option.id === selectedId}
              tabindex={option.id === selectedId ? 0 : -1}
              onclick={() => choose(option.id)}
              class="flex min-h-14 w-full items-center gap-3 rounded-lg px-2 py-2 text-left outline-none hover:bg-white/10 focus-visible:bg-white/10 aria-selected:bg-white/10 light:hover:bg-zinc-900/5 light:focus-visible:bg-zinc-900/5 light:aria-selected:bg-zinc-900/10"
            >
              {@render icon(option.name, "size-9")}
              <span class="flex min-w-0 flex-1 flex-col gap-0.5">
                <span class="flex min-w-0 items-center gap-1.5">
                  <span class="truncate text-sm font-semibold text-zinc-100 light:text-zinc-900">{option.name}</span>
                  {@render badges(option)}
                </span>
                <span class="truncate text-xs text-zinc-400 light:text-zinc-600">{details(option)}</span>
              </span>
              {#if option.id === selectedId}<Icon name="check" size="size-4 shrink-0 text-legio-success" />{/if}
            </button>
          {/each}
        </div>
        {#if onAdd}
          <button type="button" class="mt-1 flex h-11 w-full items-center gap-3 rounded-lg px-2 text-left text-sm text-zinc-200 outline-none hover:bg-white/10 focus-visible:bg-white/10 light:text-zinc-800 light:hover:bg-zinc-900/5 light:focus-visible:bg-zinc-900/5" onclick={() => { open = false; onAdd(); }}>
            <span class="flex size-9 shrink-0 items-center justify-center"><Icon name="plus" size="size-5" /></span>{t("Aggiungi un'altra versione", $language)}
          </button>
        {/if}
      </div>

      {#if showLinks}
        <div data-group class="border-t border-white/10 pt-1 light:border-zinc-900/10">
          <p id="{panelId}-links" class="px-2 pt-1 pb-1.5 text-xs font-medium text-zinc-400 light:text-zinc-600">{t("Collegamento di download", $language)}</p>
          <div role="radiogroup" aria-labelledby="{panelId}-links" class="flex flex-wrap gap-1.5 px-2 pb-2">
            {#each linkOptions as option (option.value)}
              <button
                type="button"
                role="radio"
                aria-checked={option.value === link}
                tabindex={option.value === link ? 0 : -1}
                disabled={linksDisabled}
                onclick={() => { link = option.value; close(); }}
                class="flex h-8 max-w-full items-center gap-1.5 rounded-full border border-white/10 px-3 text-sm text-zinc-200 outline-none transition-colors hover:bg-white/10 focus-visible:bg-white/10 disabled:cursor-not-allowed disabled:opacity-50 aria-checked:border-white/30 aria-checked:bg-white/15 aria-checked:text-white light:border-zinc-900/10 light:text-zinc-800 light:hover:bg-zinc-900/5 light:focus-visible:bg-zinc-900/5 light:aria-checked:border-zinc-900/30 light:aria-checked:bg-zinc-900/10 light:aria-checked:text-zinc-950"
              >
                {#if option.value === link}<Icon name="check" size="size-3.5 shrink-0 text-legio-success" />{/if}
                <span class="min-w-0 truncate">{option.label}</span>
              </button>
            {/each}
          </div>
        </div>
      {/if}
    </div>
  {/if}
</div>
