<script lang="ts">
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import { activeDownloadCount, downloads } from "../../stores/downloads";
  import { gameCount, games, steamGameCount } from "../../stores/games";
  import { network, networkSummary } from "../../stores/network";
  import { source } from "../../stores/source";

  const verifiedCount = $derived($source.data.manifest?.verified.length ?? 0);
  const unverifiedCount = $derived($source.data.manifest?.unverified.length ?? 0);
  const hasManifest = $derived($source.data.manifest !== null);

  const isLoading = $derived(
    $games.status === "loading" ||
      $downloads.status === "loading" ||
      $source.status === "loading" ||
      $network.status === "loading",
  );

  const cards = $derived([
    { label: "Giochi in libreria", value: $gameCount.toString(), hint: `${$steamGameCount} da Steam` },
    {
      label: "Download attivi",
      value: $activeDownloadCount.toString(),
      hint: `${$downloads.data.length} in coda`,
    },
    { label: "Stato rete", value: $networkSummary.status, hint: `Steam: ${$networkSummary.steam}` },
    {
      label: "Sorgente Legio",
      value: hasManifest ? `${verifiedCount + unverifiedCount}` : "Non disponibile",
      hint: hasManifest
        ? `${verifiedCount} verificate, ${unverifiedCount} non verificate`
        : "Manifest non scaricato",
    },
  ]);
</script>

<div class="grid gap-4 sm:grid-cols-2">
  {#each cards as card (card.label)}
    <div class="rounded-xl bg-white/5 p-5 light:bg-zinc-100">
      <p class="text-sm text-zinc-400 light:text-zinc-600">{card.label}</p>
      <p class="mt-2 text-3xl font-semibold text-zinc-50 light:text-zinc-900">{card.value}</p>
      <p class="mt-1 text-xs text-zinc-500">{card.hint}</p>
    </div>
  {/each}
</div>

{#if isLoading}
  <p class="mt-6 text-sm text-zinc-500" role="status">Caricamento dello stato...</p>
{/if}

{#if $games.status === "error" && $games.error !== null}
  <div class="mt-6">
    <ErrorBanner message={`Libreria: ${$games.error}`} onRetry={() => void games.load()} />
  </div>
{/if}

{#if $downloads.status === "error" && $downloads.error !== null}
  <div class="mt-4">
    <ErrorBanner message={`Download: ${$downloads.error}`} onRetry={() => void downloads.load()} />
  </div>
{/if}
