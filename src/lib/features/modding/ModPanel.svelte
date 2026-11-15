<script lang="ts">
  import { tick } from "svelte";
  import { fade } from "svelte/transition";
  import { t, language } from "../../i18n";
  import Icon from "../../components/ui/Icon.svelte";
  import IconButton from "../../components/ui/IconButton.svelte";
  import type { ModUpdate, ProfileMod } from "../../services/modding";
  import { reducedMotion } from "../../utils/motion";
  import {
    adjacentModTab,
    closeModTab,
    focusModTab,
    modTabTitle,
    openModTab,
    setModPanelCollapsed,
    setModTabSummary,
    type ModTabs,
  } from "./mod-panel-model";
  import ModIcon from "./ModIcon.svelte";
  import ModTabPage from "./ModTabPage.svelte";

  let {
    ecosystemGameId,
    tabs,
    installed,
    updates,
    disabled,
    openOnInstalled,
    onChange,
    onInstall,
  }: {
    ecosystemGameId: string;
    tabs: ModTabs;
    installed: ReadonlyMap<string, ProfileMod>;
    updates: ReadonlyMap<string, ModUpdate>;
    disabled: boolean;
    openOnInstalled: boolean;
    onChange: (tabs: ModTabs) => void;
    onInstall: (fullName: string, version: string | null) => void;
  } = $props();

  let strip = $state<HTMLDivElement>();

  const behavior = reducedMotion ? "auto" : "smooth";

  // Keeps the shown tab inside the strip without scrolling any other container.
  $effect(() => {
    void tabs.active;
    void tabs.collapsed;
    const current = strip?.querySelector('[aria-selected="true"]')?.parentElement;
    if (strip === undefined || current === null || current === undefined) return;
    const bounds = strip.getBoundingClientRect();
    const box = current.getBoundingClientRect();
    const overflow = box.left < bounds.left ? box.left - bounds.left : box.right > bounds.right ? box.right - bounds.right : 0;
    if (overflow !== 0) strip.scrollBy?.({ left: overflow, behavior });
  });

  // A plain wheel scrolls the strip sideways.
  function onWheel(event: WheelEvent): void {
    if (strip === undefined || event.deltaX !== 0 || event.deltaY === 0 || strip.scrollWidth <= strip.clientWidth) return;
    event.preventDefault();
    strip.scrollLeft += event.deltaY;
  }

  async function show(key: string | null): Promise<void> {
    if (key === null) return;
    onChange(focusModTab(tabs, key));
    await tick();
    document.getElementById(`mod-tab-${key}`)?.focus();
  }

  function onKeydown(event: KeyboardEvent, key: string): void {
    const target = event.key === "ArrowRight" ? adjacentModTab(tabs, key, 1)
      : event.key === "ArrowLeft" ? adjacentModTab(tabs, key, -1)
        : event.key === "Home" ? (tabs.tabs[0]?.key ?? null)
          : event.key === "End" ? (tabs.tabs.at(-1)?.key ?? null)
            : undefined;
    if (target !== undefined) {
      event.preventDefault();
      void show(target);
    } else if (event.key === "Delete") {
      event.preventDefault();
      const next = closeModTab(tabs, key);
      onChange(next);
      void show(next.active);
    }
  }
</script>

<div class="flex min-w-0 flex-col items-center lg:relative lg:overflow-hidden">
  {#if tabs.tabs.length > 0}
    <aside
      class="absolute inset-y-0 right-0 z-20 flex w-full max-w-lg flex-col overflow-hidden rounded-2xl legio-glass shadow-2xl transition-opacity duration-200 motion-reduce:transition-none lg:w-md lg:max-w-none lg:shadow-none xl:w-lg {tabs.collapsed ? 'pointer-events-none opacity-0' : ''}"
      aria-label={t("Mod aperte", $language)}
      inert={tabs.collapsed}
      transition:fade={{ duration: reducedMotion ? 0 : 200 }}
    >
      <div class="flex shrink-0 items-center gap-1 border-b border-legio-line-subtle p-1.5">
        <div bind:this={strip} role="tablist" aria-label={t("Mod aperte", $language)} class="flex min-w-0 flex-1 gap-1 overflow-x-auto" onwheel={onWheel}>
          {#each tabs.tabs as tab (tab.key)}
            {@const selected = tab.key === tabs.active}
            {@const title = modTabTitle(tab)}
            <div class="flex w-44 shrink-0 items-center gap-0.5 rounded-lg pr-1 transition-colors {selected ? 'bg-white/15 text-legio-fg light:bg-white light:shadow-sm' : 'text-legio-fg-muted hover:bg-legio-fill hover:text-legio-fg'}">
              <button
                type="button"
                role="tab"
                id="mod-tab-{tab.key}"
                aria-selected={selected}
                aria-controls="mod-tabpanel-{tab.key}"
                tabindex={selected ? 0 : -1}
                {title}
                class="flex min-w-0 flex-1 items-center gap-2 rounded-lg py-1.5 pl-2 text-left text-sm focus-visible:outline-2 focus-visible:outline-(--legio-focus)"
                onclick={() => onChange(focusModTab(tabs, tab.key))}
                onmousedown={(event) => { if (event.button === 1) event.preventDefault(); }}
                onauxclick={(event) => { if (event.button === 1) onChange(closeModTab(tabs, tab.key)); }}
                onkeydown={(event) => onKeydown(event, tab.key)}
              >
                <ModIcon source={tab.summary?.icon ?? null} class="size-5 rounded" />
                <span class="min-w-0 flex-1 truncate">{title}</span>
              </button>
              <IconButton size="xs" label={t("Chiudi {0}", $language, [title])} onClick={() => onChange(closeModTab(tabs, tab.key))}>
                <Icon name="close" size="size-3.5" />
              </IconButton>
            </div>
          {/each}
        </div>
        <IconButton size="sm" label={t("Comprimi le mod aperte", $language)} title={t("Comprimi le mod aperte", $language)} onClick={() => onChange(setModPanelCollapsed(tabs, true))}>
          <Icon name="next" size="size-4" />
        </IconButton>
      </div>
      {#each tabs.tabs as tab (tab.key)}
        <div
          role="tabpanel"
          id="mod-tabpanel-{tab.key}"
          aria-labelledby="mod-tab-{tab.key}"
          hidden={tab.key !== tabs.active}
          class="min-h-0 flex-1 overflow-y-auto"
        >
          <ModTabPage
            {ecosystemGameId}
            {tab}
            {installed}
            {updates}
            {disabled}
            {openOnInstalled}
            {onInstall}
            onOpen={(fullName) => onChange(openModTab(tabs, fullName))}
            onSummary={(summary) => onChange(setModTabSummary(tabs, summary))}
          />
        </div>
      {/each}
    </aside>
  {/if}
  {#if tabs.collapsed}
    <div
      class="flex w-full flex-1 flex-col items-center gap-1 rounded-2xl py-1.5 legio-glass"
      transition:fade={{ duration: reducedMotion ? 0 : 200 }}
    >
      <IconButton size="sm" label={t("Mostra le mod aperte", $language)} title={t("Mostra le mod aperte", $language)} onClick={() => onChange(setModPanelCollapsed(tabs, false))}>
        <Icon name="previous" size="size-4" />
      </IconButton>
      <span class="text-xs tabular-nums text-legio-fg-muted" aria-hidden="true">{tabs.tabs.length}</span>
    </div>
  {/if}
</div>
