<script lang="ts">
  import { t, language } from "../../i18n";
  import FilterMenu from "../../components/ui/FilterMenu.svelte";
  import Icon from "../../components/ui/Icon.svelte";
  import type { CatalogAvailability, CatalogSort } from "../../services/catalog";

  let { sort, availability, disabled = false, onChange }: {
    sort: CatalogSort;
    availability: CatalogAvailability;
    disabled?: boolean;
    onChange: (changes: { storeSort?: CatalogSort; storeAvailability?: CatalogAvailability }) => void;
  } = $props();

  const sorts = $derived<{ value: CatalogSort; label: string }[]>([
    { value: "relevance", label: t("Più affine", $language) },
    { value: "name_asc", label: t("Alfabetico crescente (A-Z)", $language) },
    { value: "name_desc", label: t("Alfabetico decrescente (Z-A)", $language) },
  ]);
  const availabilities = $derived<{ value: CatalogAvailability; label: string }[]>([
    { value: "all", label: t("Tutti", $language) },
    { value: "available", label: t("Solo disponibili", $language) },
    { value: "verified", label: t("Solo verified", $language) },
  ]);
</script>

<FilterMenu active={sort !== "relevance" || availability !== "all"}>
  {@render group(t("Ordina per", $language), sorts, sort, (value) => onChange({ storeSort: value }), false)}
  {@render group(t("Disponibilità", $language), availabilities, availability, (value) => onChange({ storeAvailability: value }), true)}
</FilterMenu>

{#snippet group<T extends string>(label: string, options: { value: T; label: string }[], current: T, select: (value: T) => void, divided: boolean)}
  <div role="radiogroup" aria-label={label} class="flex flex-col {divided ? 'border-t border-legio-line-subtle pt-1' : ''}">
    <span class="px-2 py-1 text-xs text-legio-fg-subtle">{label}</span>
    {#each options as option (option.value)}
      {@const selected = current === option.value}
      <button
        type="button"
        role="radio"
        aria-checked={selected}
        {disabled}
        onclick={() => select(option.value)}
        class="flex w-full items-center justify-between gap-2 rounded-control px-2 py-1.5 text-left text-sm text-legio-fg hover:bg-legio-fill-hover focus-visible:-outline-offset-2 focus-visible:bg-legio-fill-hover"
      >
        <span class="min-w-0 break-words">{option.label}</span>
        {#if selected}<Icon name="check" class="shrink-0 text-legio-success" />{/if}
      </button>
    {/each}
  </div>
{/snippet}
