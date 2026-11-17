<script lang="ts">
  import { onMount } from "svelte";
  import { startVisibleRefresh } from "../../stores/visible-refresh";
  import { t, language } from "../../i18n";
  import { formatDate } from "../../utils/format";
  import { changelog, loadChangelog } from "../../stores/changelog";
  import { settings } from "../../stores/settings";
  import { openReleaseNotes } from "../../stores/update-notes";
  import { openGameLink } from "../library/open-link";
  import EmptyState from "../../components/ui/EmptyState.svelte";
  import Button from "../../components/ui/Button.svelte";
  import RefreshIcon from "../../components/ui/RefreshIcon.svelte";
  import IconButton from "../../components/ui/IconButton.svelte";
  import Panel from "../../components/ui/Panel.svelte";
  import Callout from "../../components/ui/Callout.svelte";

  let refreshing = $state(false);
  const releases = $derived($changelog.data.releases);
  const enabled = $derived($settings.data.automaticUpdateCheck);
  const date = (value: string) => formatDate(value, $language, { dateStyle: "medium" });
  onMount(() => startVisibleRefresh(() => void loadChangelog(), 60 * 60 * 1000));

  async function refresh(): Promise<void> {
    refreshing = true;
    try { await loadChangelog(true); } finally { refreshing = false; }
  }
</script>

<Panel title={t("Changelog", $language)} class="min-h-0">
  {#snippet actions()}
    <IconButton disabled={!enabled} label={t("Aggiorna il changelog", $language)} title={t("Aggiorna", $language)} onClick={() => void refresh()}><RefreshIcon spinning={refreshing} /></IconButton>
  {/snippet}
  <div class="min-h-0 overflow-auto [mask-image:linear-gradient(to_bottom,black_calc(100%-1.25rem),transparent)]">
    {#if releases.length > 0}
      <ul class="flex flex-col divide-y divide-legio-line-subtle">
        {#each releases as release (release.url)}
          <li class="flex flex-col gap-1 py-3">
            <div class="flex items-baseline justify-between gap-2">
              <span class="text-sm font-semibold text-legio-fg">{release.version}{#if release.prerelease}<span class="ml-2 text-xs font-normal text-legio-fg-muted">{t("Anteprima", $language)}</span>{/if}</span>
              <time class="text-xs text-legio-fg-muted" datetime={release.publishedAt}>{date(release.publishedAt)}</time>
            </div>
            {#if release.notes}<p class="line-clamp-3 whitespace-pre-line break-words text-xs text-legio-fg-muted">{release.notes}</p>{/if}
            <div class="flex gap-2">
              {#if release.notes}<Button label={t("Mostra di più", $language)} variant="link" size="sm" onClick={() => openReleaseNotes({ version: release.version, notes: release.notes })} />{/if}
              <Button label={t("Apri su GitHub", $language)} variant="link" size="sm" onClick={() => void openGameLink(release.url)} />
            </div>
          </li>
        {/each}
      </ul>
    {:else if !enabled}
      <EmptyState icon="globe" title={t("Changelog disattivato", $language)} description={t("Si aggiorna insieme al controllo automatico degli aggiornamenti in Impostazioni, Privacy e rete.", $language)} />
    {:else}
      <p class="text-sm text-legio-fg-muted" role="status">{$changelog.status === "idle" || $changelog.status === "loading" || refreshing ? t("Caricamento del changelog...", $language) : t("Nessuna versione disponibile.", $language)}</p>
    {/if}
    {#if $changelog.data.warning || $changelog.error}<Callout tone="warning" variant="inline" class="mt-3" message={t("Non è stato possibile aggiornare il changelog. Le versioni salvate restano disponibili.", $language)} />{/if}
  </div>
</Panel>
