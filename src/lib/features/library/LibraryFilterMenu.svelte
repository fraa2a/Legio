<script lang="ts">
  import { tick } from "svelte";
  import { t, language } from "../../i18n";
  import Icon from "../../components/ui/Icon.svelte";
  import Toggle from "../../components/ui/Toggle.svelte";
  import Tooltip from "../../components/ui/Tooltip.svelte";
  import type { PlayStatus } from "../../services/library-organization";
  import { reducedMotion } from "../../utils/motion";
  import { playStatusOptions } from "./library-labels";
  import { hasActiveFilters, type LibraryFilters } from "./library-organize";

  let { filters, onChange }: { filters: LibraryFilters; onChange: (changes: Partial<LibraryFilters>) => void } = $props();

  const panelId = $props.id();
  const duration = reducedMotion ? 0 : 200;
  // Matches --legio-ease in app.css.
  const easing = "cubic-bezier(0.2, 0, 0, 1)";
  const collapsed = { height: "0px" };
  const openSurface = "legio-popover shadow-xl shadow-black/30 light:shadow-zinc-900/15";

  let root: HTMLDivElement;
  let trigger: HTMLButtonElement;
  let panel = $state<HTMLDivElement>();
  let open = $state(false);
  let closing = $state(false);
  let panelTop = $state(0);
  let panelLeft = $state(0);

  const expanded = $derived(open && !closing);
  const active = $derived(hasActiveFilters(filters));
  const statuses = $derived([{ value: null, label: t("Tutti gli stati", $language) }, ...playStatusOptions($language)]);

  $effect(() => {
    if (!open) return;
    window.addEventListener("resize", dismiss);
    return () => window.removeEventListener("resize", dismiss);
  });

  async function show(): Promise<void> {
    if (open) return;
    open = true;
    await tick();
    if (!open || !panel) return;
    const anchor = root.getBoundingClientRect();
    panelTop = anchor.bottom;
    panelLeft = anchor.left;
    panel.showPopover();
    await tick();
    if (!open || !panel?.isConnected) return;
    if (duration > 0) panel.animate([collapsed, { height: `${panel.offsetHeight}px` }], { duration, easing });
    panel.querySelector<HTMLElement>('[aria-checked="true"]')?.focus({ preventScroll: true });
  }

  // Collapses the panel back into the funnel before removing it.
  function close(restoreFocus: boolean): void {
    if (!open || closing) return;
    if (restoreFocus) trigger.focus();
    if (duration === 0 || !panel) {
      open = false;
      return;
    }
    closing = true;
    const animation = panel.animate([{ height: `${panel.offsetHeight}px` }, collapsed], { duration, easing, fill: "forwards" });
    animation.onfinish = dismiss;
  }

  // Closes at once when the window resizes and the panel would no longer line up.
  function dismiss(): void {
    open = false;
    closing = false;
  }

  function handleKeydown(event: KeyboardEvent): void {
    if (event.key !== "Escape" || !expanded) return;
    event.preventDefault();
    event.stopPropagation();
    close(true);
  }
</script>

<svelte:window onclick={(event) => {
  if (open && !root?.contains(event.target as Node)) close(false);
}} onfocusin={(event) => {
  if (open && !root?.contains(event.target as Node)) close(false);
}} />

<!-- The funnel box and the panel form one surface: the box becomes the tab the panel hangs from. -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  bind:this={root}
  onkeydown={handleKeydown}
  class="relative flex h-control-md shrink-0 items-center p-1 {open ? `rounded-t-card ${openSurface}` : 'legio-glass rounded-card'}"
>
  <Tooltip text={t("Filtri", $language)}><button
    bind:this={trigger}
    type="button"
    aria-label={active ? t("Filtri, attivi", $language) : t("Filtri", $language)}
    aria-haspopup="dialog"
    aria-expanded={expanded}
    aria-controls={panelId}
    onclick={() => expanded ? close(true) : void show()}
    class="relative inline-flex size-control-sm items-center justify-center rounded-control text-legio-fg-muted transition-colors hover:bg-legio-fill-hover hover:text-legio-fg {expanded ? 'bg-legio-fill-hover text-legio-fg' : ''}"
  >
    <Icon name="funnel" />
    {#if active}
      <span aria-hidden="true" class="absolute top-1 right-1 size-1.5 rounded-full bg-legio-accent"></span>
    {/if}
  </button></Tooltip>
  {#if open}
    <div
      bind:this={panel}
      id={panelId}
      popover="manual"
      role="dialog"
      tabindex="-1"
      aria-label={t("Filtri", $language)}
      style:top="{panelTop}px"
      style:left="{panelLeft}px"
      style:right="auto"
      class="fixed m-0 flex w-60 max-w-[calc(100vw-1rem)] flex-col gap-1 overflow-hidden rounded-b-card rounded-tr-card p-2 text-legio-fg focus:outline-none {openSurface}"
    >
      <div class="px-1 py-1.5">
        <Toggle label={t("Solo preferiti", $language)} checked={filters.favoritesOnly} onChange={(favoritesOnly) => onChange({ favoritesOnly })} />
      </div>
      <div role="radiogroup" aria-label={t("Stato", $language)} class="flex flex-col border-t border-legio-line-subtle pt-1">
        {#each statuses as status (status.value ?? "all")}
          {@const selected = filters.playStatus === status.value}
          <button
            type="button"
            role="radio"
            aria-checked={selected}
            onclick={() => onChange({ playStatus: status.value as PlayStatus | null })}
            class="flex w-full items-center justify-between gap-2 rounded-control px-2 py-1.5 text-left text-sm text-legio-fg hover:bg-legio-fill-hover focus-visible:-outline-offset-2 focus-visible:bg-legio-fill-hover"
          >
            <span class="min-w-0 break-words">{status.label}</span>
            {#if selected}<Icon name="check" class="shrink-0 text-legio-success" />{/if}
          </button>
        {/each}
      </div>
    </div>
  {/if}
</div>
