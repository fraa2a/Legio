<script lang="ts">
  import { t, language } from "../../i18n";
  import Panel from "../../components/ui/Panel.svelte";
  import { renderSteamDescription } from "./steam-description";

  let {
    requirements,
    loading = false,
    hasSteamAppId = true,
  }: {
    requirements: { minimum: string | null; recommended: string | null } | null;
    loading?: boolean;
    hasSteamAppId?: boolean;
  } = $props();
</script>

<Panel title={t("Requisiti di sistema", $language)}>
  {#if loading && requirements === null}
    <p class="text-sm text-zinc-400 light:text-zinc-600" role="status">{t("Caricamento requisiti Steam...", $language)}</p>
  {:else if !hasSteamAppId}
    <p class="text-sm text-zinc-400 light:text-zinc-600">{t("Associa uno Steam App ID al gioco per caricare i requisiti.", $language)}</p>
  {:else if requirements === null || (!requirements.minimum && !requirements.recommended)}
    <p class="text-sm text-zinc-400 light:text-zinc-600">{t("Steam non pubblica requisiti di sistema per questo gioco.", $language)}</p>
  {/if}
  {#if requirements?.minimum}
    <div class="space-y-2">
      <h4 class="text-sm font-medium text-zinc-200 light:text-zinc-800">{t("Minimi", $language)}</h4>
      <div use:renderSteamDescription={requirements.minimum} class="space-y-2 text-sm leading-relaxed text-zinc-400 [&_li]:ml-4 [&_ul]:list-disc light:text-zinc-600"></div>
    </div>
  {/if}
  {#if requirements?.recommended}
    <div class="space-y-2">
      <h4 class="text-sm font-medium text-zinc-200 light:text-zinc-800">{t("Consigliati", $language)}</h4>
      <div use:renderSteamDescription={requirements.recommended} class="space-y-2 text-sm leading-relaxed text-zinc-400 [&_li]:ml-4 [&_ul]:list-disc light:text-zinc-600"></div>
    </div>
  {/if}
</Panel>
