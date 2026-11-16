<script lang="ts">
  import { t, language } from "../../i18n";
  import Button from "../../components/ui/Button.svelte";
  import Icon from "../../components/ui/Icon.svelte";
  import IconButton from "../../components/ui/IconButton.svelte";
  import SegmentedControl from "../../components/ui/SegmentedControl.svelte";
  import SelectField from "../../components/ui/SelectField.svelte";
  import Slider from "../../components/ui/Slider.svelte";
  import type { Collection, PlayStatus } from "../../services/library-organization";
  import type { LibraryCollectionFilter, LibrarySortKey, LibraryViewMode, LibraryViewPreferences } from "../../services/local-state";
  import { playStatusOptions, sortLabel } from "./library-labels";
  import { defaultDescending } from "./library-organize";

  let {
    view,
    collections,
    onChange,
    onPreviewCoverScale,
    onCommitCoverScale,
    onManageCollections,
  }: {
    view: LibraryViewPreferences;
    collections: Collection[];
    onChange: (changes: Partial<LibraryViewPreferences>) => void;
    onPreviewCoverScale: (scale: number) => void;
    onCommitCoverScale: () => void;
    onManageCollections: () => void;
  } = $props();

  // Mirrors MIN_LIBRARY_COVER_SCALE and MAX_LIBRARY_COVER_SCALE in database.rs.
  const minCoverScale = 70;
  const maxCoverScale = 150;
  const sortKeys: LibrarySortKey[] = ["name", "playtime", "lastPlayed", "added", "origin"];

  const collectionValue = $derived(view.collection.kind === "collection" ? `collection:${view.collection.id}` : view.collection.kind);

  function parseCollection(value: string): LibraryCollectionFilter {
    if (value.startsWith("collection:")) return { kind: "collection", id: value.slice("collection:".length) };
    return value === "uncollected" ? { kind: "uncollected" } : { kind: "all" };
  }
</script>

<div class="flex flex-wrap items-center gap-2">
  <SegmentedControl
    name="library-view"
    label={t("Vista", $language)}
    labelHidden
    class="w-72"
    value={view.mode}
    options={[
      { value: "grid", label: t("Griglia", $language) },
      { value: "list", label: t("Elenco", $language) },
      { value: "shelves", label: t("Scaffali", $language) },
    ]}
    onChange={(value) => onChange({ mode: value as LibraryViewMode })}
  />
  <div class="flex items-center gap-1">
    <SelectField
      id="library-sort"
      label={t("Ordina per", $language)}
      labelHidden
      class="w-48"
      value={view.sort}
      options={sortKeys.map((key) => ({ value: key, label: sortLabel(key, $language) }))}
      onChange={(value) => { if (value !== view.sort) onChange({ sort: value as LibrarySortKey, descending: defaultDescending(value as LibrarySortKey) }); }}
    />
    <IconButton
      size="md"
      label={view.descending ? t("Ordine decrescente", $language) : t("Ordine crescente", $language)}
      title={view.descending ? t("Ordine decrescente", $language) : t("Ordine crescente", $language)}
      onClick={() => onChange({ descending: !view.descending })}
    >
      <Icon name={view.descending ? "sort-descending" : "sort-ascending"} />
    </IconButton>
  </div>
  <SelectField
    id="library-status-filter"
    label={t("Stato", $language)}
    labelHidden
    class="w-44"
    value={view.playStatus ?? "all"}
    options={[{ value: "all", label: t("Tutti gli stati", $language) }, ...playStatusOptions($language)]}
    onChange={(value) => onChange({ playStatus: value === "all" ? null : value as PlayStatus })}
  />
  <SelectField
    id="library-collection-filter"
    label={t("Raccolta", $language)}
    labelHidden
    class="w-48"
    value={collectionValue}
    options={[
      { value: "all", label: t("Tutte le raccolte", $language) },
      { value: "uncollected", label: t("Senza raccolta", $language) },
      ...collections.map((collection) => ({ value: `collection:${collection.id}`, label: collection.name })),
    ]}
    onChange={(value) => onChange({ collection: parseCollection(value) })}
  />
  <Button
    label={t("Preferiti", $language)}
    variant={view.favoritesOnly ? "secondary" : "ghost"}
    pressed={view.favoritesOnly}
    onClick={() => onChange({ favoritesOnly: !view.favoritesOnly })}
  >
    <Icon name="star" class={view.favoritesOnly ? "fill-current text-legio-warning" : ""} />
  </Button>
  <Button label={t("Raccolte", $language)} variant="ghost" onClick={onManageCollections}>
    <Icon name="collection" />
  </Button>
  {#if view.mode !== "list"}
    <div class="ml-auto flex items-center gap-2 text-legio-fg-muted">
      <Icon name="image" />
      <Slider
        label={t("Dimensione copertine", $language)}
        class="w-32"
        min={minCoverScale}
        max={maxCoverScale}
        step={10}
        value={view.coverScale}
        valueText={`${view.coverScale}%`}
        onInput={onPreviewCoverScale}
        onCommit={onCommitCoverScale}
      />
    </div>
  {/if}
</div>
