<script lang="ts">
  import { onMount } from "svelte";
  import { flip } from "svelte/animate";
  import { cubicOut } from "svelte/easing";
  import { fade, fly } from "svelte/transition";
  import { t, language } from "../../i18n";
  import Button from "../../components/ui/Button.svelte";
  import EmptyState from "../../components/ui/EmptyState.svelte";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import Icon from "../../components/ui/Icon.svelte";
  import Panel from "../../components/ui/Panel.svelte";
  import SegmentedControl from "../../components/ui/SegmentedControl.svelte";
  import SelectField from "../../components/ui/SelectField.svelte";
  import Skeleton from "../../components/ui/Skeleton.svelte";
  import { games } from "../../stores/games";
  import { createRecapResource, recapYears } from "../../stores/playtime-recap";
  import { showToast } from "../../stores/toast";
  import { toMessage } from "../../utils/errors";
  import { regionalLocale } from "../../utils/format";
  import { fadeDuration, pageTransitionDuration } from "../../utils/motion";
  import type { PlaytimeRecap } from "../../services/playtime";
  import { formatPlaytime } from "../home/home-model";
  import GameIcon from "../library/GameIcon.svelte";
  import RecapBarChart from "./RecapBarChart.svelte";
  import TweenedText from "./TweenedText.svelte";
  import { exportRecapCard } from "./recap-export";
  import {
    chartBars,
    dayLabels,
    defaultRecapSelection,
    formatRecapPeriod,
    formatShare,
    monthNames,
    monthOptions,
    showsDayLabel,
    weekdayNames,
    yearOptions,
    type RecapSelection,
  } from "./recap-model";
  import { recapTiles } from "./recap-stats";

  const locale = $derived(regionalLocale($language));
  let selection = $state<RecapSelection | null>(null);
  let exporting = $state(false);

  const resource = $derived(selection === null ? null : createRecapResource(selection));
  const recap = $derived(resource === null ? null : $resource);
  const loaded = $derived(recap?.status === "ready" ? recap.data : null);
  // The previous period stays on screen while the next one loads.
  let kept = $state<PlaytimeRecap | null>(null);
  const data = $derived(loaded ?? kept);
  const loading = $derived(recap?.status === "loading");
  const years = $derived($recapYears.data);

  const modeOptions = $derived([
    { value: "year", label: t("Anno", $language) },
    { value: "month", label: t("Mese", $language) },
  ]);
  const tiles = $derived(data === null ? [] : recapTiles(data, locale, $language));
  const weekdayBars = $derived(data === null ? [] : chartBars(data.byWeekday, weekdayNames(locale, "short"), formatPlaytime));
  const bucketBars = $derived.by(() => {
    if (data === null || selection === null) return [];
    const labels = selection.mode === "year" ? monthNames(locale, "short") : dayLabels(data.byBucket.length);
    return chartBars(data.byBucket, labels, formatPlaytime);
  });

  onMount(() => { void recapYears.load(); });

  $effect(() => {
    if (selection === null && $recapYears.status === "ready") selection = defaultRecapSelection($recapYears.data, new Date());
  });

  $effect(() => { void resource?.load(); });

  $effect(() => {
    if (loaded !== null) kept = loaded;
    else if (recap?.status === "empty") kept = null;
  });

  function select(change: Partial<RecapSelection>): void {
    if (selection !== null) selection = { ...selection, ...change };
  }

  async function exportCard(): Promise<void> {
    if (selection === null || loaded === null || exporting) return;
    exporting = true;
    try {
      if (await exportRecapCard(selection, loaded, $games.data, $language)) showToast(t("Scheda salvata", $language), "success");
    } catch (error) {
      showToast(t("Impossibile salvare la scheda: {0}", $language, [toMessage(error)]), "error");
    } finally {
      exporting = false;
    }
  }
</script>

<div class="@container flex min-h-full flex-col gap-4">
  {#if $recapYears.status === "error"}
    <ErrorBanner message={$recapYears.error ?? ""} onRetry={() => void recapYears.load()} />
  {:else if $recapYears.status === "empty"}
    <EmptyState icon="calendar" title={t("Nessuna partita registrata", $language)} description={t("Quando avrai giocato, qui troverai il riepilogo del tuo anno.", $language)} />
  {:else if selection === null}
    <Skeleton class="h-control-md" />
    <div class="grid gap-4 sm:grid-cols-2 xl:grid-cols-3" role="status">
      <span class="sr-only">{t("Caricamento dell'annata...", $language)}</span>
      {#each Array.from({ length: 6 }, (_, tile) => tile) as tile (tile)}<Skeleton class="h-24" />{/each}
    </div>
  {:else}
    <div class="flex flex-wrap items-end justify-between gap-4">
      <div class="min-w-0">
        <h1 class="text-2xl font-semibold text-legio-fg">{formatRecapPeriod(selection, locale)}</h1>
        <p class="mt-0.5 text-sm text-legio-fg-muted">{t("Annata di gioco", $language)}</p>
      </div>
      <div class="flex flex-wrap items-end gap-2">
        <SegmentedControl name="recap-mode" label={t("Periodo", $language)} labelHidden class="shrink-0" options={modeOptions} value={selection.mode} onChange={(value) => select({ mode: value === "month" ? "month" : "year" })} />
        <SelectField id="recap-year" label={t("Anno", $language)} labelHidden class="w-28" value={String(selection.year)} options={yearOptions(years)} onChange={(value) => select({ year: Number(value) })} />
        {#if selection.mode === "month"}
          <SelectField id="recap-month" label={t("Mese", $language)} labelHidden class="w-40" value={String(selection.month)} options={monthOptions(locale)} onChange={(value) => select({ month: Number(value) })} />
        {/if}
        <Button label={exporting ? t("Salvataggio...", $language) : t("Esporta scheda", $language)} variant="secondary" disabled={loaded === null || exporting} onClick={() => void exportCard()}>
          <Icon name="image" />
        </Button>
      </div>
    </div>

    {#if recap?.status === "error"}
      <ErrorBanner message={recap.error ?? ""} onRetry={() => void resource?.load()} />
    {:else if recap?.status === "empty"}
      <EmptyState icon="calendar" title={t("Nessuna partita in questo periodo", $language)} description={t("Non ci sono sessioni di gioco registrate. Scegli un altro periodo.", $language)} />
    {:else if data === null}
      <div class="grid gap-4 sm:grid-cols-2 xl:grid-cols-3" role="status">
        <span class="sr-only">{t("Caricamento dell'annata...", $language)}</span>
        {#each Array.from({ length: 6 }, (_, tile) => tile) as tile (tile)}<Skeleton class="h-24" />{/each}
      </div>
    {:else}
      <dl class="grid gap-4 transition-opacity duration-(--legio-motion) sm:grid-cols-2 xl:grid-cols-3 {loading ? 'opacity-60' : ''}" aria-busy={loading}>
        {#each tiles as tile (tile.key)}
          <div class="flex min-w-0 flex-col gap-1 legio-glass rounded-card p-4">
            <dt class="text-sm font-medium text-legio-fg-muted">{tile.label}</dt>
            <dd class="truncate text-2xl font-semibold tabular-nums text-legio-fg">
              {#if tile.amount === null}{tile.value}{:else}<TweenedText amount={tile.amount} format={tile.format} />{/if}
            </dd>
            {#if tile.note !== null}<dd class="truncate text-xs text-legio-fg-muted">{tile.note}</dd>{/if}
          </div>
        {/each}
      </dl>

      <Panel title={t("Giochi più giocati", $language)}>
        <ol class="flex flex-col gap-4">
          {#each data.topGames as entry (entry.gameId)}
            {@const game = $games.data.find((candidate) => candidate.id === entry.gameId)}
            <li class="flex items-center gap-4" animate:flip={{ duration: pageTransitionDuration, easing: cubicOut }} in:fly={{ y: 8, duration: pageTransitionDuration, easing: cubicOut }} out:fade={{ duration: fadeDuration }}>
              {#if game !== undefined}
                <GameIcon {game} class="size-10 shrink-0 rounded-control object-contain" />
              {:else}
                <span class="grid size-10 shrink-0 place-items-center rounded-control bg-legio-fill-hover text-sm font-semibold text-legio-fg" aria-hidden="true">{entry.name.trim().charAt(0).toLocaleUpperCase() || "?"}</span>
              {/if}
              <div class="flex min-w-0 flex-1 flex-col gap-1.5">
                <div class="flex items-baseline justify-between gap-3">
                  <span class="truncate text-sm font-medium text-legio-fg">{entry.name}</span>
                  <span class="shrink-0 text-xs text-legio-fg-muted tabular-nums"><TweenedText amount={entry.milliseconds} format={formatPlaytime} /> - <TweenedText amount={entry.share} format={(share) => formatShare(share, locale)} /></span>
                </div>
                <div class="h-2 overflow-hidden rounded-full bg-legio-fill-hover" role="img" aria-label={t("{0} del tempo di gioco", $language, [formatShare(entry.share, locale)])}>
                  <div class="h-full rounded-full bg-legio-accent transition-[width] duration-(--legio-motion-slow)" style:width="{Math.max(1, entry.share * 100)}%"></div>
                </div>
              </div>
            </li>
          {/each}
        </ol>
      </Panel>

      <div class="grid gap-4 @min-[48rem]:grid-cols-2">
        <Panel title={t("Per giorno della settimana", $language)}>
          <RecapBarChart title={t("Per giorno della settimana", $language)} bars={weekdayBars} />
        </Panel>
        <Panel title={selection.mode === "year" ? t("Per mese", $language) : t("Per giorno", $language)}>
          <RecapBarChart
            title={selection.mode === "year" ? t("Per mese", $language) : t("Per giorno", $language)}
            bars={bucketBars}
            labelVisible={(index) => selection?.mode === "year" || showsDayLabel(index + 1, bucketBars.length)}
          />
        </Panel>
      </div>
    {/if}
  {/if}
</div>
