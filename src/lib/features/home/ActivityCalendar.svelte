<script lang="ts">
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
  const monthLabel = $derived(calendar.first.toLocaleDateString("it-IT", { month: "long", year: "numeric" }));
  const total = $derived($activity.data?.reduce((sum, value) => sum + value, 0) ?? 0);
  const playedDays = $derived($activity.data?.filter((value) => value > 0).length ?? 0);

  $effect(() => { void activity.load(); });
  onMount(() => {
    const timer = setInterval(() => {
      now = new Date();
      void activity.load();
    }, 30000);
    return () => clearInterval(timer);
  });
</script>

<section class="rounded-2xl bg-zinc-900 p-5 light:bg-zinc-100" aria-labelledby="activity-title">
  <div class="mb-5 flex items-center justify-between gap-3">
    <div>
      <h2 id="activity-title" class="text-lg font-medium text-zinc-50 light:text-zinc-900">Attività di gioco</h2>
      <p class="mt-1 text-sm capitalize text-zinc-400 light:text-zinc-600">{monthLabel}</p>
    </div>
    <div class="flex gap-1">
      <Button label="Mese precedente" variant="secondary" circle onClick={() => monthOffset--}><Icon name="previous" /></Button>
      <Button label="Mese successivo" variant="secondary" circle disabled={monthOffset >= 0} onClick={() => monthOffset++}><Icon name="next" /></Button>
    </div>
  </div>

  <StateBlock status={$activity.status} hasData={$activity.data !== null} error={$activity.error} onRetry={() => void activity.load()} loadingMessage="Caricamento attività..." />

  {#if $activity.data !== null}
    <div class="mb-3 flex flex-wrap gap-4 text-xs text-zinc-400 light:text-zinc-600" aria-label="Legenda ore giocate">
      {#each [{ hours: 0, label: '0 h' }, { hours: 1, label: '< 2 h' }, { hours: 4, label: '4 h' }, { hours: 8, label: '8 h o più' }] as level (level.hours)}
        <span class="flex items-center gap-1.5"><span class="activity-cell size-3 rounded-sm" class:hatched={level.hours > 0 && level.hours < 2} style={`--intensity: ${activityIntensity(level.hours * 3600000) * 100}%`} aria-hidden="true"></span>{level.label}</span>
      {/each}
    </div>
    <div class="grid grid-cols-7 gap-2" role="group" aria-label={`Calendario di ${monthLabel}`}>
      {#each ['Lun', 'Mar', 'Mer', 'Gio', 'Ven', 'Sab', 'Dom'] as weekday (weekday)}
        <span class="pb-1 text-center text-xs text-zinc-500">{weekday}</span>
      {/each}
      {#each calendar.cells as day, index (index)}
        {#if day === null}
          <span aria-hidden="true"></span>
        {:else}
          {@const milliseconds = $activity.data[day - 1] ?? 0}
          {@const future = calendar.boundaries[day - 1] > now.getTime()}
          {@const today = new Date(calendar.boundaries[day - 1]).toDateString() === now.toDateString()}
          {@const label = `${day} ${monthLabel}: ${future ? 'giorno futuro' : formatPlaytime(milliseconds) + ' giocati'}`}
          <span
            class="activity-cell flex aspect-square items-center justify-center rounded-lg text-xs font-medium tabular-nums text-zinc-200 light:text-zinc-800"
            class:hatched={milliseconds > 0 && milliseconds < 2 * 3600000}
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
    <div class="mt-5 flex items-baseline justify-between gap-3 text-sm">
      <span class="font-medium text-zinc-50 light:text-zinc-900">{formatPlaytime(total)} giocate</span>
      <span class="text-zinc-400 light:text-zinc-600">{playedDays} giorni attivi</span>
    </div>
  {/if}
</section>

<style>
  .activity-cell {
    background-color: color-mix(in srgb, var(--color-legio-activity) var(--intensity), var(--activity-base, #3f3f46));
  }
  .hatched {
    background-image: repeating-linear-gradient(135deg, transparent 0 3px, var(--color-legio-activity) 3px 4px);
  }
  .active { color: #18181b; }
  .future { color: #a1a1aa; }
  .today { outline: 2px solid var(--color-legio-activity); outline-offset: 2px; }
  :global([data-theme="light"]) .activity-cell { --activity-base: #d4d4d8; }
  :global([data-theme="light"]) .future { color: #52525b; }
</style>
