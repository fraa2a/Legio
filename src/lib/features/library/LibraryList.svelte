<script lang="ts">
  import { onMount } from "svelte";
  import { t, language } from "../../i18n";
  import Icon from "../../components/ui/Icon.svelte";
  import IconButton from "../../components/ui/IconButton.svelte";
  import type { LibrarySortKey } from "../../services/local-state";
  import { diskUsage, watchDiskUsage } from "../../stores/disk-usage";
  import { formatBytes, formatDate } from "../../utils/format";
  import type { LibraryEntry } from "./library-organize";
  import { formatPlaytime, originLabel, playStatusLabel, sortLabel } from "./library-labels";

  let {
    entries,
    sort,
    descending,
    onSort,
    onOpen,
    onToggleFavorite,
    onOrganize,
  }: {
    entries: LibraryEntry[];
    sort: LibrarySortKey;
    descending: boolean;
    onSort: (key: LibrarySortKey) => void;
    onOpen: (entry: LibraryEntry) => void;
    onToggleFavorite: (entry: LibraryEntry) => void;
    onOrganize: (entry: LibraryEntry) => void;
  } = $props();

  const columns: { key: LibrarySortKey; numeric: boolean }[] = [
    { key: "name", numeric: false },
    { key: "playtime", numeric: true },
    { key: "lastPlayed", numeric: true },
    { key: "origin", numeric: false },
  ];

  // Only measurements already cached are shown; measuring stays in the disk usage dialog and game page.
  const sizes = $derived(new Map($diskUsage.data.games.map((entry) => [entry.gameId, entry])));
  onMount(() => watchDiskUsage(false));

  function ariaSort(key: LibrarySortKey): "ascending" | "descending" | "none" {
    if (key !== sort) return "none";
    return descending ? "descending" : "ascending";
  }
</script>

<div class="legio-glass overflow-x-auto rounded-card">
  <table class="w-full text-left text-sm">
    <caption class="sr-only">{t("Giochi in libreria", $language)}</caption>
    <thead class="border-b border-legio-line text-xs text-legio-fg-muted">
      <tr>
        <th scope="col" class="w-10 px-2 py-2"><span class="sr-only">{t("Preferito", $language)}</span></th>
        {#each columns as column (column.key)}
          <th scope="col" aria-sort={ariaSort(column.key)} class="px-2 py-1 font-medium {column.numeric ? 'text-right' : ''}">
            <button
              type="button"
              class="inline-flex items-center gap-1.5 rounded-control px-2 py-1 hover:bg-legio-fill-hover hover:text-legio-fg {column.key === sort ? 'text-legio-fg' : ''}"
              onclick={() => onSort(column.key)}
            >
              {sortLabel(column.key, $language)}
              {#if column.key === sort}
                <Icon name={descending ? "sort-descending" : "sort-ascending"} size="sm" />
              {/if}
            </button>
          </th>
          {#if column.key === "name"}
            <th scope="col" class="px-4 py-2 font-medium">{t("Stato", $language)}</th>
          {/if}
        {/each}
        <th scope="col" class="px-4 py-2 text-right font-medium">{t("Spazio su disco", $language)}</th>
        <th scope="col" class="w-10 px-2 py-2"><span class="sr-only">{t("Azioni", $language)}</span></th>
      </tr>
    </thead>
    <tbody>
      {#each entries as entry (entry.game.id)}
        {@const size = sizes.get(entry.game.id)}
        <tr class="border-b border-legio-line-subtle last:border-b-0 hover:bg-legio-fill">
          <td class="px-2 py-1">
            <IconButton
              label={entry.favorite ? t("Rimuovi dai preferiti", $language) : t("Aggiungi ai preferiti", $language)}
              pressed={entry.favorite}
              onClick={() => onToggleFavorite(entry)}
            >
              <Icon name="star" class={entry.favorite ? "fill-current text-legio-warning" : ""} />
            </IconButton>
          </td>
          <th scope="row" class="max-w-0 px-2 py-1 font-medium">
            <button type="button" class="block w-full truncate rounded-control px-2 py-1 text-left text-legio-fg hover:underline" onclick={() => onOpen(entry)}>
              {entry.game.name}
            </button>
          </th>
          <td class="px-4 py-1 text-legio-fg-muted">{entry.playStatus === "none" ? "" : playStatusLabel(entry.playStatus, $language)}</td>
          <td class="px-4 py-1 text-right tabular-nums text-legio-fg-muted">{formatPlaytime(entry.totalMilliseconds)}</td>
          <td class="px-4 py-1 text-right tabular-nums text-legio-fg-muted">
            {entry.lastPlayedAt === null ? t("Mai giocato", $language) : formatDate(entry.lastPlayedAt, $language)}
          </td>
          <td class="px-4 py-1 text-legio-fg-muted">{originLabel(entry.origin, $language)}</td>
          <td class="px-4 py-1 text-right tabular-nums text-legio-fg-muted">
            {#if size !== undefined}{formatBytes(size.bytes, $language)}{size.partial ? ` (${t("parziale", $language)})` : ""}{/if}
          </td>
          <td class="px-2 py-1">
            <IconButton label={t("Organizza {0}", $language, [entry.game.name])} onClick={() => onOrganize(entry)}>
              <Icon name="collection" />
            </IconButton>
          </td>
        </tr>
      {/each}
    </tbody>
  </table>
</div>
