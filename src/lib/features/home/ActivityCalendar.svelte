<script lang="ts">
  import { t, language } from "../../i18n";
  import { onMount } from "svelte";
  import Button from "../../components/ui/Button.svelte";
  import Icon from "../../components/ui/Icon.svelte";
  import StateBlock from "../../components/ui/StateBlock.svelte";
  import { getPlaytimeActivity } from "../../services/playtime";
  import { createResource } from "../../stores/resource";
  import { activityIntensity, formatPlaytime, monthCalendar } from "./home-model";

  let now = $state(new Date());
  let monthOffset = $state(0);
  const currentMonth = $derived(`${now.getFullYear()}-${now.getMonth()}`);
  const calendar = $derived.by(() => {
    const [year, month] = currentMonth.split("-").map(Number);
    return monthCalendar(year, month + monthOffset);
  });
  const activity = $derived.by(() => {
    const boundaries = calendar.boundaries;
    return createResource<number[] | null>(null, () => getPlaytimeActivity(boundaries));
  });
  const monthLabel = $derived(calendar.first.toLocaleDateString(undefined, { month: "long", year: "numeric" }));

  $effect(() => { void activity.load(); });
  onMount(() => {
    const timer = setInterval(() => {
      now = new Date();
      void activity.load();
    }, 30000);
    return () => clearInterval(timer);
  });
</script>

<section class="legio-glass flex min-h-0 flex-col rounded-2xl bg-zinc-900 p-4 light:bg-zinc-100" aria-labelledby="activity-title">
  <div class="mb-3 flex shrink-0 items-center justify-between gap-3">
    <div>
      <h2 id="activity-title" class="text-lg font-medium text-zinc-50 light:text-zinc-900">{t("Attività di gioco", $language)}</h2>
      <p class="mt-1 text-sm capitalize text-zinc-400 light:text-zinc-600">{monthLabel}</p>
    </div>
    <div class="flex gap-1">
      <Button label={t("Mese precedente", $language)} variant="secondary" circle onClick={() => monthOffset--}><Icon name="previous" /></Button>
      <Button label={t("Mese successivo", $language)} variant="secondary" circle disabled={monthOffset >= 0} onClick={() => monthOffset++}><Icon name="next" /></Button>
    </div>
  </div>

  <StateBlock status={$activity.status} hasData={$activity.data !== null} error={$activity.error} onRetry={() => void activity.load()} loadingMessage={t("Caricamento attività...", $language)} />

  {#if $activity.data !== null}
    <div class="mb-3 flex shrink-0 items-center gap-3 text-xs text-zinc-400 light:text-zinc-600" role="img" aria-label={t("Intensità proporzionale alle ore giocate, da 0 a 8 ore o più", $language)}>
      <span>0 h</span>
      <span class="activity-scale block h-2 flex-1 rounded-full" aria-hidden="true"></span>
      <span>{t("8 h o più", $language)}</span>
    </div>
    <div class="grid shrink-0 grid-cols-7 gap-1.5" aria-hidden="true">
      {#each Array.from({ length: 7 }, (_, day) => new Date(2026, 0, 5 + day).toLocaleDateString(undefined, { weekday: "short" })) as weekday (weekday)}
        <span class="pb-1 text-center text-xs text-zinc-500">{weekday}</span>
      {/each}
    </div>
    <div class="grid min-h-0 flex-1 auto-rows-fr grid-cols-7 gap-1.5" role="group" aria-label={t("Calendario di {0}", $language, [monthLabel])}>
      {#each calendar.cells as day, index (index)}
        {#if day === null}
          <span aria-hidden="true"></span>
        {:else}
          {@const milliseconds = $activity.data[day - 1] ?? 0}
          {@const future = calendar.boundaries[day - 1] > now.getTime()}
          {@const today = new Date(calendar.boundaries[day - 1]).toDateString() === now.toDateString()}
          {@const label = `${day} ${monthLabel}: ${future ? t("giorno futuro", $language) : formatPlaytime(milliseconds) + t(" giocati", $language)}`}
          <span
            class="activity-cell flex min-h-0 items-center justify-center rounded-lg text-xs font-medium tabular-nums text-zinc-200 light:text-zinc-800"
            class:future
            class:today
            class:active={milliseconds >= 3 * 3600000}
            style={`--intensity: ${activityIntensity(milliseconds) * 100}%`}
            title={label}
            role="img"
            aria-label={label}
          >{day}</span>
        {/if}
      {/each}
    </div>
  {/if}
</section>

<style>
  .activity-cell {
    background-color: color-mix(in srgb, var(--color-legio-activity) var(--intensity), var(--activity-base, #3f3f46));
  }
  .activity-scale {
    background: linear-gradient(to right, var(--activity-base, #3f3f46), var(--color-legio-activity));
  }
  .active { color: #18181b; }
  .future { color: #a1a1aa; }
  .today { outline: 2px solid var(--color-legio-activity); outline-offset: 2px; }
  :global([data-theme="light"]) .activity-cell,
  :global([data-theme="light"]) .activity-scale { --activity-base: #d4d4d8; }
  :global([data-theme="light"]) .future { color: #52525b; }
</style>
