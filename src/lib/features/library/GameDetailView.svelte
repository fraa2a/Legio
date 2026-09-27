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
  import GameSettingsDialog from "./GameSettingsDialog.svelte";
  import GameLaunchControls from "./GameLaunchControls.svelte";
  import ArtworkViewer from "./ArtworkViewer.svelte";
  import SteamMetadata from "./SteamMetadata.svelte";
  import SteamArtwork from "./SteamArtwork.svelte";
  import SystemRequirements from "./SystemRequirements.svelte";
  import Icon from "../../components/ui/Icon.svelte";
  import { getGameBanner, getGameIcon, gameArtworkRevision } from "../../services/game-artwork";
  import { toMessage } from "../../utils/errors";

  const game = $derived($games.data.find((entry) => entry.id === $selectedGameId) ?? null);
  const steamAppId = $derived(game?.steamAppId ?? null);
  const isSteamGame = $derived(game !== null && game.steamInstallPath !== null);
  const launch = $derived(game === null ? undefined : $launchStateByGame.get(game.id));
  const detailsState = $derived(steamAppId === null ? null : ($steamDetails[steamAppId] ?? null));
  const details = $derived(detailsState?.details ?? null);
  const headline = $derived.by(() => {
    if (details === null) return null;
    const date = details.releaseDate;
    const release = date === null ? null : date.comingSoon ? "In arrivo" : date.date;
    return [details.appType, release].filter((part) => part !== null).join(" · ");
  });
  const location = $derived(game?.steamInstallPath ?? game?.executablePath ?? null);

  let artworkOpen = $state(false);
  let gameSettingsOpen = $state(false);
  let customBannerUrl = $state<string | null>(null);
  let customIconUrl = $state<string | null>(null);
  let artworkError = $state<string | null>(null);

  $effect(() => {
    if ($selectedGameId !== null && game === null) closeGame();
  });

  $effect(() => {
    if (steamAppId !== null) ensureSteamDetails(steamAppId);
  });

  $effect(() => {
    const currentGame = game;
    void $gameArtworkRevision;
    if (currentGame === null) return;

    let cancelled = false;
    let bannerObjectUrl: string | null = null;
    let iconObjectUrl: string | null = null;
    customBannerUrl = null;
    customIconUrl = null;
    artworkError = null;

    const asUrl = (bytes: number[], contentType: string): string =>
      URL.createObjectURL(new Blob([Uint8Array.from(bytes)], { type: contentType }));
    void Promise.all([getGameBanner(currentGame.id), getGameIcon(currentGame.id)]).then(
      ([banner, icon]) => {
        if (cancelled) return;
        bannerObjectUrl = banner === null ? null : asUrl(banner.bytes, banner.contentType);
        iconObjectUrl = icon === null ? null : asUrl(icon.bytes, icon.contentType);
        customBannerUrl = bannerObjectUrl;
        customIconUrl = iconObjectUrl;
      },
      (cause: unknown) => {
        if (!cancelled) artworkError = toMessage(cause);
      },
    );

    return () => {
      cancelled = true;
      if (bannerObjectUrl !== null) URL.revokeObjectURL(bannerObjectUrl);
      if (iconObjectUrl !== null) URL.revokeObjectURL(iconObjectUrl);
    };
  });
</script>

{#if game === null}
  <p class="rounded-xl bg-white/5 p-6 text-zinc-400 light:bg-zinc-100 light:text-zinc-600">
    Il gioco non è più disponibile.
  </p>
{:else}
  <div class="flex min-h-full flex-col gap-4">
    <section class="relative isolate w-full min-w-0 overflow-hidden rounded-2xl bg-zinc-800 light:bg-zinc-200 {customBannerUrl !== null ? 'aspect-[2.2/1]' : 'aspect-[3.1/1]'}">
      {#if customBannerUrl !== null}
        <img src={customBannerUrl} alt="Banner personalizzato di {game.name}" class="absolute inset-0 block size-full object-cover object-center" />
      {:else if details !== null}
        <SteamArtwork
          steamAppId={details.steamAppId}
          asset="hero"
          fallbackAsset="header"
          version={detailsState?.cachedAt ?? null}
          caption={false}
          class="absolute inset-0 block size-full object-cover object-center"
        >
          {#snippet placeholder()}{@render backdrop()}{/snippet}
        </SteamArtwork>
      {:else}
        {@render backdrop()}
      {/if}
      {#if details !== null && customBannerUrl === null}
        <button type="button" class="absolute inset-0 z-0 cursor-zoom-in" aria-label="Ingrandisci copertina" onclick={() => (artworkOpen = true)}></button>
      {/if}
      <div class="pointer-events-none absolute inset-0 z-[1] bg-gradient-to-t from-zinc-950/95 via-zinc-950/35 to-transparent"></div>
      <div class="absolute inset-x-0 bottom-0 z-10 flex flex-wrap items-end justify-between gap-4 p-5 sm:p-7">
        <div class="pointer-events-none flex min-w-0 flex-col items-start gap-2">
          {#if launch?.status === "running"}
            <Badge tone="success" title="In esecuzione" />
          {:else if launch?.status === "launching"}
            <Badge tone="warning" title="Avvio in corso" />
          {/if}
          <h2 class="max-w-full text-2xl font-semibold text-white sm:text-3xl">
            {#if customIconUrl !== null}
              <img src={customIconUrl} alt="Icona personalizzata di {game.name}" class="max-h-20 max-w-80 object-contain object-left" />
            {:else if steamAppId !== null}
              <SteamArtwork {steamAppId} asset="logo" version={detailsState?.cachedAt ?? null} caption={false} alt="Logo di {game.name}" class="max-h-20 max-w-80 object-contain object-left">
                {#snippet placeholder()}<span class="truncate">{game.name}</span>{/snippet}
              </SteamArtwork>
            {:else}
              {game.name}
            {/if}
          </h2>
          {#if headline !== null}
            <p class="text-sm text-zinc-200">{headline}</p>
          {:else if location !== null}
            <p class="max-w-full truncate text-sm text-zinc-300" title={location}>{location}</p>
          {/if}
          {#if launch?.error}<p class="text-sm text-red-300" role="alert">{launch.error}</p>{/if}
        </div>
        <div class="flex flex-wrap gap-2">
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
          <Button label="Impostazioni del gioco" variant="secondary" onClick={() => (gameSettingsOpen = true)}>
            <Icon name="settings" size="h-4 w-4" />
          </Button>
        </div>
      </div>
    </section>

    {#if artworkError !== null}
      <ErrorBanner message={artworkError} />
    {/if}

    {#if $launchError !== null}
      <ErrorBanner message={$launchError} />
    {/if}

    <div class="grid flex-1 gap-4 xl:grid-cols-[minmax(0,1fr)_22rem]">
      <div class="min-w-0">
        {#if steamAppId !== null}<SteamMetadata {steamAppId} />{/if}
      </div>
      <div class="flex min-w-0 flex-col gap-4">
        <SystemRequirements
          requirements={details?.systemRequirements ?? null}
          loading={detailsState?.status === "loading"}
          hasSteamAppId={steamAppId !== null}
        />
        <Panel title="Info gioco">
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

{#if gameSettingsOpen && game !== null}
  <GameSettingsDialog
    {game}
    onClose={() => (gameSettingsOpen = false)}
    onRemoved={() => { gameSettingsOpen = false; closeGame(); }}
  />
{/if}

{#if artworkOpen && details !== null}
  <ArtworkViewer
    steamAppId={details.steamAppId}
    asset="hero"
    fallbackAsset="header"
    alt="Copertina di {game?.name ?? details.name}"
    onClose={() => (artworkOpen = false)}
  />
{/if}

{#if $accountSwitchGame !== null}
<Dialog
  open
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
{/if}

{#snippet backdrop()}
  <div
    class="absolute inset-0 bg-gradient-to-br from-zinc-700 to-zinc-900 light:from-zinc-300 light:to-zinc-100"
    aria-hidden="true"
  ></div>
{/snippet}
