<script lang="ts">
  import { t, language } from "../../i18n";
  import type { ProtonDbSummary } from "../../services/protondb";
  import { regionalLocale } from "../../utils/format";
  import Badge from "../../components/ui/Badge.svelte";
  import Button from "../../components/ui/Button.svelte";
  import Icon from "../../components/ui/Icon.svelte";
  import Panel from "../../components/ui/Panel.svelte";
  import Skeleton from "../../components/ui/Skeleton.svelte";
  import { openGameLink } from "./open-link";
  import { confidenceLabel, isRatedTier, tierLabel, tierMeaning } from "./protondb";

  let {
    summary,
    loading,
    url,
  }: { summary: ProtonDbSummary | null; loading: boolean; url: string } = $props();

  const trending = $derived(summary?.trendingTier != null && summary.trendingTier !== "unknown" ? summary.trendingTier : null);
  const confidence = $derived(summary?.confidence != null && summary.confidence !== "unknown" ? summary.confidence : null);
</script>

<Panel title={t("Compatibilità su Linux", $language)}>
  {#if loading}
    <div class="flex flex-col gap-2" role="status" aria-label={t("Caricamento compatibilità ProtonDB...", $language)}>
      <Skeleton class="h-6 w-32" />
      <Skeleton class="h-4 w-full" />
      <Skeleton class="h-4 w-2/3" />
    </div>
  {:else if summary === null}
    <p class="text-sm text-legio-fg-muted">{t("ProtonDB non ha ancora segnalazioni per questo gioco.", $language)}</p>
  {:else}
    <div class="flex flex-col gap-2">
      <div>
        {#if isRatedTier(summary.tier)}
          <Badge tone="tier-{summary.tier}" title={t(tierLabel[summary.tier], $language)} />
        {:else}
          <Badge title={t(tierLabel[summary.tier], $language)} />
        {/if}
      </div>
      <p class="text-sm text-legio-fg">{t(tierMeaning[summary.tier], $language)}</p>
    </div>
    <dl class="grid gap-2 text-sm">
      {#if confidence !== null}
        <div class="flex flex-wrap gap-x-3">
          <dt class="w-32 shrink-0 text-legio-fg-muted">{t("Affidabilità", $language)}</dt>
          <dd class="min-w-0 flex-1 text-legio-fg">{t(confidenceLabel[confidence], $language)}</dd>
        </div>
      {/if}
      <div class="flex flex-wrap gap-x-3">
        <dt class="w-32 shrink-0 text-legio-fg-muted">{t("Segnalazioni", $language)}</dt>
        <dd class="min-w-0 flex-1 text-legio-fg">{summary.totalReports.toLocaleString(regionalLocale($language))}</dd>
      </div>
      {#if trending !== null}
        <div class="flex flex-wrap gap-x-3">
          <dt class="w-32 shrink-0 text-legio-fg-muted">{t("Tendenza recente", $language)}</dt>
          <dd class="min-w-0 flex-1 text-legio-fg">{t(tierLabel[trending], $language)}</dd>
        </div>
      {/if}
    </dl>
  {/if}
  {#if !loading}
    <div>
      <Button label={t("Apri su ProtonDB", $language)} variant="link" size="sm" onClick={() => void openGameLink(url)}>
        <Icon name="external-link" size="sm" />
      </Button>
    </div>
  {/if}
</Panel>
