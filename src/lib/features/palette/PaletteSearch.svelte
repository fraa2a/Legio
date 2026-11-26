<script lang="ts">
  import { tick } from "svelte";
  import { t, language } from "../../i18n";
  import Icon, { type IconName } from "../../components/ui/Icon.svelte";
  import { reducedMotion } from "../../utils/motion";
  import { appInfo } from "../../stores/app-info";
  import { diskUsageDialogOpen } from "../../stores/disk-usage";
  import { shortcutsOpen } from "../../stores/shortcuts";
  import { games } from "../../stores/games";
  import { addGameDialogOpen, storeQuery } from "../../stores/library-ui";
  import { openGame, openPlaytimeRecap, openSettings, sectionLabels, sections, selectSection, type Section } from "../../stores/navigation";
  import { settingsCategories } from "../settings/settings-categories";
  import { rankPaletteItems, type PaletteItem } from "./palette-model";

  type Entry = PaletteItem & { icon: IconName; run: () => void };

  let { id }: { id: string } = $props();

  const listId = $props.id();
  const sectionIcons: Record<Section, IconName> = { home: "home", library: "library", store: "store", mods: "mods", downloads: "downloads" };
  const duration = reducedMotion ? 0 : 200;
  // Matches --legio-ease in app.css.
  const easing = "cubic-bezier(0.2, 0, 0, 1)";
  const collapsed = { height: "0px", paddingTop: "0px", paddingBottom: "0px" };

  let root: HTMLDivElement;
  let list = $state<HTMLDivElement>();
  let input = $state<HTMLInputElement>();
  let animation: Animation | undefined;
  let query = $state("");
  let active = $state(0);
  let open = $state(false);
  let closing = $state(false);
  const expanded = $derived(open && !closing);

  const entries: Entry[] = $derived([
    ...sections.map((section): Entry => ({ id: `section:${section}`, kind: "section", label: t(sectionLabels[section], $language), hint: t("Pagina", $language), icon: sectionIcons[section], run: () => selectSection(section) })),
    ...$games.data.map((game): Entry => ({ id: `game:${game.id}`, kind: "game", label: game.name, hint: t("Libreria", $language), icon: "play", run: () => openGame(game.id) })),
    { id: "action:add-game", kind: "action", label: t("Aggiungi gioco", $language), hint: t("Libreria", $language), icon: "plus", run: () => { selectSection("library"); addGameDialogOpen.set(true); } },
    { id: "action:disk-usage", kind: "action", label: t("Cosa occupa spazio", $language), hint: t("Libreria", $language), icon: "folder", keywords: "disco spazio dimensione pulizia disk space size cleanup", run: () => diskUsageDialogOpen.set(true) },
    { id: "action:playtime-recap", kind: "action", label: t("Annata di gioco", $language), hint: t(sectionLabels.home, $language), icon: "calendar", keywords: "recap statistiche tempo di gioco", run: openPlaytimeRecap },
    { id: "action:shortcuts", kind: "action", label: t("Scorciatoie", $language), hint: t("Aiuto", $language), icon: "keyboard", keywords: "tastiera shortcuts keyboard", run: () => shortcutsOpen.set(true) },
    ...settingsCategories($appInfo.data.platform).map((category): Entry => ({ id: `setting:${category.id}`, kind: "setting", label: t(category.label, $language), hint: t("Impostazioni", $language), icon: category.icon, run: () => openSettings(category.id) })),
  ]);
  const results: Entry[] = $derived([
    ...rankPaletteItems(entries, query),
    ...(query.trim() === "" ? [] : [{ id: "action:store-search", kind: "action", label: t("Cerca nello Store: {0}", $language, [query.trim()]), hint: t("Store", $language), icon: "search", run: () => { storeQuery.set(query.trim()); selectSection("store"); } } satisfies Entry]),
  ]);
  const selected = $derived(results[Math.min(active, results.length - 1)]);

  $effect(() => {
    if (selected === undefined || !expanded) return;
    void tick().then(() => document.getElementById(`${listId}-${selected.id}`)?.scrollIntoView?.({ block: "nearest" }));
  });

  // The list grows out of the input as one surface and collapses back into it.
  async function show(): Promise<void> {
    if (expanded) return;
    animation?.cancel();
    animation = undefined;
    closing = false;
    if (open) return;
    open = true;
    await tick();
    if (duration > 0 && list) animation = list.animate([collapsed, { height: `${list.offsetHeight}px` }], { duration, easing });
  }

  function hide(): void {
    if (!open || closing) return;
    if (duration === 0 || !list) {
      open = false;
      return;
    }
    closing = true;
    animation = list.animate([{ height: `${list.offsetHeight}px` }, collapsed], { duration, easing, fill: "forwards" });
    animation.onfinish = () => {
      open = false;
      closing = false;
    };
  }

  function choose(entry: Entry): void {
    // The store search entry reads the query, so it runs before the field is cleared.
    entry.run();
    query = "";
    active = 0;
    hide();
    input?.blur();
  }

  function handleKeydown(event: KeyboardEvent): void {
    if (event.key === "Escape" && expanded) {
      event.preventDefault();
      hide();
    } else if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      if (!expanded) void show();
      else if (results.length > 0) active = (active + (event.key === "ArrowDown" ? 1 : results.length - 1)) % results.length;
    } else if (event.key === "Enter" && expanded && selected !== undefined) {
      event.preventDefault();
      choose(selected);
    }
  }

  function hideOutside(event: Event): void {
    if (!root.contains(event.target as Node)) hide();
  }
</script>

<svelte:window onclick={hideOutside} onfocusin={hideOutside} />

<div bind:this={root} class="@container relative w-full min-w-0">
  <label for={id} class="sr-only">{t("Cerca giochi, pagine e impostazioni", $language)}</label>
  <div class="flex h-control-md items-center gap-2.5 px-3 text-legio-fg-subtle {expanded ? 'legio-popover rounded-t-card shadow-xl shadow-black/30 light:shadow-zinc-900/15' : 'legio-glass rounded-card'}">
    <span class="shrink-0"><Icon name="search" /></span>
    <input
      {id}
      bind:this={input}
      bind:value={query}
      type="text"
      role="combobox"
      aria-expanded={expanded}
      aria-autocomplete="list"
      aria-controls={listId}
      aria-activedescendant={expanded && selected !== undefined ? `${listId}-${selected.id}` : undefined}
      aria-keyshortcuts="Control+K"
      placeholder={t("Cerca ovunque", $language)}
      autocomplete="off"
      spellcheck="false"
      onfocus={() => void show()}
      onclick={() => void show()}
      oninput={() => { active = 0; void show(); }}
      onkeydown={handleKeydown}
      class="h-full min-w-0 flex-1 bg-transparent text-sm text-legio-fg placeholder:text-legio-fg-subtle"
    />
    {#if query === ""}<kbd class="hidden shrink-0 text-xs font-medium @min-[18rem]:inline">Ctrl K</kbd>{/if}
  </div>
  {#if open}
    <!-- Keeping focus in the input lets a click choose an entry without blurring it first. -->
    <div
      bind:this={list}
      id={listId}
      role="listbox"
      tabindex="-1"
      aria-label={t("Risultati", $language)}
      onpointerdown={(event) => event.preventDefault()}
      class="legio-popover absolute inset-x-0 top-full z-50 flex max-h-80 flex-col gap-0.5 overflow-y-auto rounded-b-card border-t border-legio-line-subtle p-1 shadow-xl shadow-black/30 light:shadow-zinc-900/15"
    >
      {#each results as entry, index (entry.id)}
        <button
          id="{listId}-{entry.id}"
          type="button"
          role="option"
          tabindex="-1"
          aria-selected={index === Math.min(active, results.length - 1)}
          onpointermove={() => { active = index; }}
          onclick={() => choose(entry)}
          class="flex h-control-md w-full shrink-0 items-center gap-3 rounded-control px-3 text-left text-sm text-legio-fg aria-selected:bg-legio-fill-hover aria-selected:text-legio-fg"
        >
          <span class="shrink-0 text-legio-fg-muted"><Icon name={entry.icon} /></span>
          <span class="min-w-0 flex-1 truncate">{entry.label}</span>
          {#if entry.hint}<span class="shrink-0 text-xs text-legio-fg-subtle">{entry.hint}</span>{/if}
        </button>
      {:else}
        <p class="px-3 py-6 text-center text-sm text-legio-fg-subtle" role="status">{t("Nessun risultato", $language)}</p>
      {/each}
    </div>
  {/if}
</div>
