<script lang="ts">
  import { t, language } from "../../i18n";
  import Button from "../../components/ui/Button.svelte";
  import Dialog from "../../components/ui/Dialog.svelte";
  import Icon from "../../components/ui/Icon.svelte";
  import SegmentedControl from "../../components/ui/SegmentedControl.svelte";
  import Toggle from "../../components/ui/Toggle.svelte";
  import type { CategoryMode } from "../../services/modding";
  import { activeFilterCount, categoryState, clearFilterOptions, cycleCategory, type BrowserFilters, type CategoryState } from "./modding-model";

  let {
    open,
    filters,
    categories,
    onChange,
    onClose,
  }: {
    open: boolean;
    filters: BrowserFilters;
    categories: string[];
    onChange: (filters: BrowserFilters) => void;
    onClose: () => void;
  } = $props();

  const modeOptions = $derived([
    { value: "any", label: t("Almeno una", $language) },
    { value: "all", label: t("Tutte", $language) },
  ]);

  function chipLabel(category: string, state: CategoryState): string {
    if (state === "included") return t("{0}: inclusa", $language, [category]);
    if (state === "excluded") return t("{0}: esclusa", $language, [category]);
    return category;
  }

  function chipClass(state: CategoryState): string {
    if (state === "included") return "bg-legio-accent text-legio-accent-text";
    if (state === "excluded") return "bg-legio-error/15 text-legio-error line-through";
    return "bg-legio-fill-hover text-legio-fg hover:bg-legio-fill-active";
  }
</script>

<Dialog {open} title={t("Filtri", $language)} size="wide" {onClose}>
  {#snippet children(close)}
    <div class="flex flex-col gap-5">
      <div class="flex flex-col gap-3">
        <Toggle label={t("Mostra mod deprecate", $language)} checked={filters.includeDeprecated} onChange={(value) => onChange({ ...filters, includeDeprecated: value })} />
        <Toggle label={t("Mostra contenuti NSFW", $language)} checked={filters.includeNsfw} onChange={(value) => onChange({ ...filters, includeNsfw: value })} />
      </div>

      <div class="flex flex-col gap-2">
        <h3 class="text-sm font-medium text-legio-fg">{t("Categorie", $language)}</h3>
        <p class="text-xs text-legio-fg-subtle">{t("Tocca una categoria per includerla, ancora per escluderla, una terza volta per rimuoverla.", $language)}</p>
        {#if filters.categories.length > 1}
          <SegmentedControl
            name="mod-category-mode"
            label={t("Una mod deve avere", $language)}
            options={modeOptions}
            value={filters.categoryMode}
            onChange={(value) => onChange({ ...filters, categoryMode: value as CategoryMode })}
          />
        {/if}
        {#if categories.length === 0}
          <p class="text-sm text-legio-fg-muted">{t("Nessuna categoria disponibile.", $language)}</p>
        {:else}
          <ul class="flex max-h-72 flex-wrap gap-2 overflow-y-auto">
            {#each categories as category (category)}
              {@const state = categoryState(filters, category)}
              <li>
                <button
                  type="button"
                  class="inline-flex items-center gap-1 rounded-full px-3 py-1 text-sm transition-colors focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-(--legio-focus) {chipClass(state)}"
                  aria-label={chipLabel(category, state)}
                  aria-pressed={state !== "neutral"}
                  onclick={() => onChange(cycleCategory(filters, category))}
                >
                  {#if state === "included"}<Icon name="check" size="size-3.5" />{:else if state === "excluded"}<Icon name="close" size="size-3.5" />{/if}
                  {category}
                </button>
              </li>
            {/each}
          </ul>
        {/if}
      </div>

      <div class="flex justify-end gap-2">
        <Button label={t("Azzera i filtri", $language)} variant="secondary" disabled={activeFilterCount(filters) === 0} onClick={() => onChange(clearFilterOptions(filters))} />
        <Button label={t("Chiudi", $language)} onClick={close} />
      </div>
    </div>
  {/snippet}
</Dialog>
