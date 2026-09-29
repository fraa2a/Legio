<script lang="ts">
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

<Panel title="Requisiti di sistema">
  {#if loading && requirements === null}
    <p class="text-sm text-zinc-400 light:text-zinc-600" role="status">Caricamento requisiti Steam...</p>
  {:else if !hasSteamAppId}
    <p class="text-sm text-zinc-400 light:text-zinc-600">Associa uno Steam App ID al gioco per caricare i requisiti.</p>
  {:else if requirements === null || (!requirements.minimum && !requirements.recommended)}
    <p class="text-sm text-zinc-400 light:text-zinc-600">Steam non pubblica requisiti di sistema per questo gioco.</p>
  {/if}
  {#if requirements?.minimum}
    <div class="space-y-2">
      <h4 class="text-sm font-medium text-zinc-200 light:text-zinc-800">Minimi</h4>
      <div use:renderSteamDescription={requirements.minimum} class="space-y-2 text-sm leading-relaxed text-zinc-400 [&_li]:ml-4 [&_ul]:list-disc light:text-zinc-600"></div>
    </div>
  {/if}
  {#if requirements?.recommended}
    <div class="space-y-2">
      <h4 class="text-sm font-medium text-zinc-200 light:text-zinc-800">Consigliati</h4>
      <div use:renderSteamDescription={requirements.recommended} class="space-y-2 text-sm leading-relaxed text-zinc-400 [&_li]:ml-4 [&_ul]:list-disc light:text-zinc-600"></div>
    </div>
  {/if}
</Panel>
