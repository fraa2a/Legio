<script lang="ts">
  import { games } from "../../stores/games";
  import {
    abortGameLaunch,
    accountSwitchGame,
    cancelPendingGameId,
    dismissAccountSwitch,
    confirmAccountSwitch,
    launchError,
    launchStateByGame,
    pendingGameId,
    playGame,
    stopGameProcess,
  } from "../../stores/launch";
  import { closeGame, selectedGameId } from "../../stores/navigation";
  import { ensureSteamDetails, steamDetails } from "../../stores/steam-details";
  import Badge from "../../components/ui/Badge.svelte";
  import Button from "../../components/ui/Button.svelte";
  import Dialog from "../../components/ui/Dialog.svelte";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import Panel from "../../components/ui/Panel.svelte";
  import GameDetailsPanel from "./GameDetailsPanel.svelte";
  import GameLaunchControls from "./GameLaunchControls.svelte";
  import ArtworkViewer from "./ArtworkViewer.svelte";
  import ExecutablePanel from "./ExecutablePanel.svelte";
  import SteamMetadata from "./SteamMetadata.svelte";
  import SteamArtwork from "./SteamArtwork.svelte";

  const game = $derived($games.data.find((entry) => entry.id === $selectedGameId) ?? null);
  const steamAppId = $derived(game?.steamAppId ?? null);
  const isSteamGame = $derived(steamAppId !== null);
  const launch = $derived(game === null ? undefined : $launchStateByGame.get(game.id));
  const detailsState = $derived(steamAppId === null ? null : ($steamDetails[steamAppId] ?? null));
  const details = $derived(detailsState?.details ?? null);
  const headline = $derived.by(() => {
    if (details === null) return null;
    const date = details.releaseDate;
    const release = date === null ? null : date.comingSoon ? "In arrivo" : date.date;
    return [details.appType, release].filter((part) => part !== null).join(" · ");
  });
  const platforms = $derived.by(() => {
    if (details?.platforms === null || details?.platforms === undefined) return [];
    return [
      details.platforms.windows ? "Windows" : null,
      details.platforms.mac ? "macOS" : null,
      details.platforms.linux ? "Linux" : null,
    ].filter((entry) => entry !== null);
  });
  const location = $derived(game?.steamInstallPath ?? game?.executablePath ?? null);

  let artworkOpen = $state(false);

  $effect(() => {
    if ($selectedGameId !== null && game === null) closeGame();
  });

  $effect(() => {
    if (steamAppId !== null) ensureSteamDetails(steamAppId);
  });
</script>

{#if game === null}
  <p class="rounded-xl bg-white/5 p-6 text-zinc-400 light:bg-zinc-100 light:text-zinc-600">
    Il gioco non è più disponibile.
  </p>
{:else}
  <div class="flex min-h-full flex-col gap-4">
    <div class="flex flex-col overflow-hidden rounded-2xl bg-white/5 light:bg-zinc-100 sm:flex-row">
      <div class="flex min-w-0 flex-1 flex-col gap-3 p-5 sm:justify-center sm:p-6">
        <div class="flex flex-wrap items-center gap-2">
          <Badge tone={isSteamGame ? "info" : "neutral"} title={isSteamGame ? "Steam" : "Manuale"} />
          {#if game.nameOverride !== null}
            <Badge tone="warning" title="Nome personalizzato" />
          {/if}
          {#if launch?.status === "running"}
            <Badge tone="success" title="In esecuzione" />
          {:else if launch?.status === "launching"}
            <Badge tone="warning" title="Avvio in corso" />
          {/if}
          {#each platforms as platform (platform)}
            <Badge tone="neutral" title={platform} />
          {/each}
        </div>
        <h2 class="text-2xl font-semibold text-zinc-50 sm:text-3xl light:text-zinc-900">
          {game.name}
        </h2>
        {#if headline !== null}
          <p class="text-sm text-zinc-400 light:text-zinc-600">{headline}</p>
        {:else if location !== null}
          <p class="truncate text-sm text-zinc-400 light:text-zinc-600" title={location}>{location}</p>
        {/if}
        {#if launch?.error}
          <p class="text-sm text-red-300 light:text-red-700" role="alert">{launch.error}</p>
        {/if}
        <div class="mt-1 flex flex-wrap gap-2">
          <GameLaunchControls
            {game}
            {launch}
            variant="primary"
            actionPending={$pendingGameId === game.id}
            cancelPending={$cancelPendingGameId === game.id}
            onPlay={playGame}
            onCancel={abortGameLaunch}
            onStop={stopGameProcess}
          />
        </div>
      </div>
      <div class="w-full shrink-0 sm:w-[30rem]">
        <div
          class="relative aspect-[2.14/1] w-full overflow-hidden bg-zinc-800 [mask-image:linear-gradient(to_bottom,transparent,#000_4rem)] light:bg-zinc-200 sm:[mask-image:linear-gradient(to_right,transparent,#000_6rem)]"
        >
          {#if details !== null}
            <SteamArtwork
              steamAppId={details.steamAppId}
              asset="header"
              version={detailsState?.cachedAt ?? null}
              caption={false}
              class="absolute inset-0 size-full object-cover"
            >
              {#snippet placeholder()}
                {@render backdrop()}
              {/snippet}
            </SteamArtwork>
          {:else}
            {@render backdrop()}
          {/if}
          {#if details !== null}
            <button
              type="button"
              class="absolute inset-0 cursor-zoom-in"
              aria-label="Ingrandisci copertina"
              onclick={() => (artworkOpen = true)}
            ></button>
          {/if}
        </div>
      </div>
    </div>

    {#if $launchError !== null}
      <ErrorBanner message={$launchError} />
    {/if}

    <div class="grid flex-1 gap-4 xl:grid-cols-[minmax(0,1fr)_22rem]">
      <div class="flex min-w-0 flex-col gap-3">
        {#if steamAppId !== null}
          <SteamMetadata {steamAppId} />
        {/if}

        {#if !isSteamGame}
          <ExecutablePanel game={game} />
        {/if}
      </div>

      <div class="flex min-w-0 flex-col gap-3">
        <GameDetailsPanel {game} onRemoved={closeGame} />

        <Panel title="Installazione" class="flex-1">
          <dl class="grid gap-2 text-sm">
            <div class="flex flex-wrap gap-x-3">
              <dt class="w-40 shrink-0 text-zinc-400 light:text-zinc-600">Origine</dt>
              <dd class="min-w-0 flex-1 text-zinc-100 light:text-zinc-900">
                {isSteamGame ? "Libreria Steam" : "Voce manuale"}
              </dd>
            </div>
            <div class="flex flex-wrap gap-x-3">
              <dt class="w-40 shrink-0 text-zinc-400 light:text-zinc-600">Steam App ID</dt>
              <dd class="min-w-0 flex-1 break-words text-zinc-100 light:text-zinc-900">
                {game.steamAppId ?? "nessuno"}
              </dd>
            </div>
            <div class="flex flex-wrap gap-x-3">
              <dt class="w-40 shrink-0 text-zinc-400 light:text-zinc-600">
                {isSteamGame ? "Cartella" : "Eseguibile"}
              </dt>
              <dd class="min-w-0 flex-1 break-all text-zinc-100 light:text-zinc-900">
                {location ?? "non impostato"}
              </dd>
            </div>
            {#if game.steamAccountId !== null}
              <div class="flex flex-wrap gap-x-3">
                <dt class="w-40 shrink-0 text-zinc-400 light:text-zinc-600">Account Steam</dt>
                <dd class="min-w-0 flex-1 break-words text-zinc-100 light:text-zinc-900">
                  {game.steamAccountId}
                </dd>
              </div>
            {/if}
            <div class="flex flex-wrap gap-x-3">
              <dt class="w-40 shrink-0 text-zinc-400 light:text-zinc-600">Nome rilevato</dt>
              <dd class="min-w-0 flex-1 break-words text-zinc-100 light:text-zinc-900">
                {game.automaticName ?? "nessuno"}
              </dd>
            </div>
          </dl>
        </Panel>
      </div>
    </div>
  </div>
{/if}

{#if artworkOpen && details !== null}
  <ArtworkViewer
    steamAppId={details.steamAppId}
    asset="header"
    alt="Copertina di {game?.name ?? details.name}"
    onClose={() => (artworkOpen = false)}
  />
{/if}

<Dialog
  open={$accountSwitchGame !== null}
  title="Cambio account Steam"
  onClose={dismissAccountSwitch}
>
  <p class="text-sm text-zinc-300 light:text-zinc-700">
    Steam deve essere chiuso e riavviato per usare l'account salvato di {$accountSwitchGame?.name}.
    Procedere?
  </p>
  <div class="flex justify-end gap-2">
    <Button label="Annulla" variant="secondary" onClick={dismissAccountSwitch} />
    <Button label="Riavvia e avvia" onClick={() => void confirmAccountSwitch()} />
  </div>
</Dialog>

{#snippet backdrop()}
  <div
    class="absolute inset-0 bg-gradient-to-br from-zinc-700 to-zinc-900 light:from-zinc-300 light:to-zinc-100"
    aria-hidden="true"
  ></div>
{/snippet}
