<script lang="ts">
  import { tick } from "svelte";
  import { t, language } from "../../i18n";
  import Dialog from "../../components/ui/Dialog.svelte";
  import Icon, { type IconName } from "../../components/ui/Icon.svelte";
  import { appInfo } from "../../stores/app-info";
  import { paletteOpen } from "../../stores/command-palette";
  import { games } from "../../stores/games";
  import { addGameDialogOpen, storeQuery } from "../../stores/library-ui";
  import { openGame, openSettings, sectionLabels, sections, selectSection, type Section } from "../../stores/navigation";
  import { settingsCategories } from "../settings/settings-categories";
  import { rankPaletteItems, type PaletteItem } from "./palette-model";

  type Entry = PaletteItem & { icon: IconName; run: () => void };

  const listId = $props.id();
  const sectionIcons: Record<Section, IconName> = { home: "home", library: "library", store: "store", downloads: "downloads" };

  let query = $state("");
  let active = $state(0);
  let input = $state<HTMLInputElement>();

  const entries: Entry[] = $derived([
    ...sections.map((section): Entry => ({ id: `section:${section}`, kind: "section", label: t(sectionLabels[section], $language), hint: t("Pagina", $language), icon: sectionIcons[section], run: () => selectSection(section) })),
    ...$games.data.map((game): Entry => ({ id: `game:${game.id}`, kind: "game", label: game.name, hint: t("Libreria", $language), icon: "play", run: () => openGame(game.id) })),
    { id: "action:add-game", kind: "action", label: t("Aggiungi gioco", $language), hint: t("Libreria", $language), icon: "plus", run: () => { selectSection("library"); addGameDialogOpen.set(true); } },
    ...settingsCategories($appInfo.data.platform).map((category): Entry => ({ id: `setting:${category.id}`, kind: "setting", label: t(category.label, $language), hint: t("Impostazioni", $language), icon: category.icon, run: () => openSettings(category.id) })),
  ]);
  const results: Entry[] = $derived([
    ...rankPaletteItems(entries, query),
    ...(query.trim() === "" ? [] : [{ id: "action:store-search", kind: "action", label: t("Cerca nello Store: {0}", $language, [query.trim()]), hint: t("Store", $language), icon: "search", run: () => { storeQuery.set(query.trim()); selectSection("store"); } } satisfies Entry]),
  ]);
  const selected = $derived(results[Math.min(active, results.length - 1)]);

  $effect(() => {
    void input?.focus();
  });

  $effect(() => {
    if (selected === undefined) return;
    void tick().then(() => document.getElementById(`${listId}-${selected.id}`)?.scrollIntoView?.({ block: "nearest" }));
  });

  function choose(entry: Entry, close: () => void): void {
    close();
    entry.run();
  }

  function handleKeydown(event: KeyboardEvent, close: () => void): void {
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      if (results.length > 0) active = (active + (event.key === "ArrowDown" ? 1 : results.length - 1)) % results.length;
    } else if (event.key === "Enter" && selected !== undefined) {
      event.preventDefault();
      choose(selected, close);
    }
  }
</script>

<Dialog open title={t("Cerca ovunque", $language)} onClose={() => paletteOpen.set(false)}>
  {#snippet children(close)}
    <div class="flex flex-col gap-3">
      <div class="flex h-11 items-center gap-2 rounded-xl border border-white/10 bg-white/5 px-3 light:border-zinc-900/10 light:bg-white">
        <Icon name="search" size="size-4 shrink-0 text-zinc-500" />
        <input
          bind:this={input}
          bind:value={query}
          type="text"
          role="combobox"
          aria-expanded="true"
          aria-controls={listId}
          aria-activedescendant={selected === undefined ? undefined : `${listId}-${selected.id}`}
          aria-label={t("Cerca giochi, pagine e impostazioni", $language)}
          placeholder={t("Cerca giochi, pagine e impostazioni", $language)}
          autocomplete="off"
          spellcheck="false"
          oninput={() => { active = 0; }}
          onkeydown={(event) => handleKeydown(event, close)}
          class="h-full min-w-0 flex-1 bg-transparent text-sm text-zinc-100 outline-none placeholder:text-zinc-500 light:text-zinc-900"
        />
      </div>
      <div id={listId} role="listbox" aria-label={t("Risultati", $language)} class="flex max-h-80 flex-col gap-0.5 overflow-y-auto">
        {#each results as entry, index (entry.id)}
          <button
            id="{listId}-{entry.id}"
            type="button"
            role="option"
            tabindex="-1"
            aria-selected={index === Math.min(active, results.length - 1)}
            onpointermove={() => { active = index; }}
            onclick={() => choose(entry, close)}
            class="flex h-11 w-full items-center gap-3 rounded-lg px-3 text-left text-sm text-zinc-200 aria-selected:bg-white/10 aria-selected:text-white light:text-zinc-800 light:aria-selected:bg-zinc-900/10 light:aria-selected:text-zinc-950"
          >
            <span class="shrink-0 text-zinc-400 light:text-zinc-600"><Icon name={entry.icon} size="size-4" /></span>
            <span class="min-w-0 flex-1 truncate">{entry.label}</span>
            {#if entry.hint}<span class="shrink-0 text-xs text-zinc-500">{entry.hint}</span>{/if}
          </button>
        {:else}
          <p class="px-3 py-6 text-center text-sm text-zinc-500" role="status">{t("Nessun risultato", $language)}</p>
        {/each}
      </div>
    </div>
  {/snippet}
</Dialog>
