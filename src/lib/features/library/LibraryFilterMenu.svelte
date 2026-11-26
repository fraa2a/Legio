<script lang="ts">
  import { t, language } from "../../i18n";
  import FilterMenu from "../../components/ui/FilterMenu.svelte";
  import Icon from "../../components/ui/Icon.svelte";
  import Toggle from "../../components/ui/Toggle.svelte";
  import type { PlayStatus } from "../../services/library-organization";
  import { playStatusOptions } from "./library-labels";
  import { hasActiveFilters, type LibraryFilters } from "./library-organize";

  let { filters, onChange }: { filters: LibraryFilters; onChange: (changes: Partial<LibraryFilters>) => void } = $props();

  const statuses = $derived([{ value: null, label: t("Tutti gli stati", $language) }, ...playStatusOptions($language)]);
</script>

<FilterMenu active={hasActiveFilters(filters)}>
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
</FilterMenu>
