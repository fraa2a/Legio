<script lang="ts">
  import { tick } from "svelte";
  import { cubicOut } from "svelte/easing";
  import { fly } from "svelte/transition";
  import { t, language } from "../../i18n";
  import Icon from "../../components/ui/Icon.svelte";
  import IconButton from "../../components/ui/IconButton.svelte";
  import type { ModUpdate, ProfileMod } from "../../services/modding";
  import { reducedMotion } from "../../utils/motion";
  import {
    adjacentModTab,
    closeModTab,
    emptyModTabs,
    focusModTab,
    modTabTitle,
    openModTab,
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
    onChange,
    onInstall,
  }: {
    ecosystemGameId: string;
    tabs: ModTabs;
    installed: ReadonlyMap<string, ProfileMod>;
    updates: ReadonlyMap<string, ModUpdate>;
    disabled: boolean;
    onChange: (tabs: ModTabs) => void;
    onInstall: (fullName: string, version: string | null) => void;
  } = $props();

  let strip = $state<HTMLDivElement>();

  $effect(() => {
    void tabs.active;
    strip?.querySelector('[aria-selected="true"]')?.scrollIntoView?.({ block: "nearest", inline: "nearest" });
  });

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

{#if tabs.tabs.length > 0}
  <aside
    class="absolute inset-y-0 right-0 z-20 flex w-full max-w-lg flex-col overflow-hidden rounded-2xl legio-glass shadow-2xl lg:static lg:w-md lg:shrink-0 lg:shadow-none xl:w-lg"
    aria-label={t("Mod aperte", $language)}
    transition:fly={{ x: 64, duration: reducedMotion ? 0 : 220, easing: cubicOut }}
  >
    <div class="flex shrink-0 items-center gap-1 border-b border-white/10 p-1.5 light:border-zinc-900/10">
      <div bind:this={strip} role="tablist" aria-label={t("Mod aperte", $language)} class="flex min-w-0 flex-1 gap-1 overflow-x-auto scrollbar-none">
        {#each tabs.tabs as tab (tab.key)}
          {@const selected = tab.key === tabs.active}
          {@const title = modTabTitle(tab)}
          <div class="flex w-44 shrink-0 items-center gap-0.5 rounded-lg pr-1 transition-colors {selected ? 'bg-white/15 text-zinc-50 light:bg-white light:text-zinc-900 light:shadow-sm' : 'text-zinc-400 hover:bg-white/5 hover:text-zinc-100 light:text-zinc-600 light:hover:bg-zinc-900/5 light:hover:text-zinc-900'}">
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
      <IconButton size="sm" label={t("Chiudi il pannello", $language)} title={t("Chiudi il pannello", $language)} onClick={() => onChange(emptyModTabs)}>
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
          {onInstall}
          onOpen={(fullName) => onChange(openModTab(tabs, fullName))}
          onSummary={(summary) => onChange(setModTabSummary(tabs, summary))}
        />
      </div>
    {/each}
  </aside>
{/if}
