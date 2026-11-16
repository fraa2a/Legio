<script lang="ts">
  import { gameUpdates } from "../../stores/game-updates";
  import { t, language } from "../../i18n";
  import { formatDate } from "../../utils/format";
  import type { Game } from "../../services/local-state";
  import type { GameLaunchState } from "../../services/steam-accounts";
  import type { SteamAssetKind } from "../../services/steam-details";
  import ArtworkTile from "../../components/ui/ArtworkTile.svelte";
  import Badge from "../../components/ui/Badge.svelte";
  import Button from "../../components/ui/Button.svelte";
  import Icon from "../../components/ui/Icon.svelte";
  import { formatPlaytime } from "./library-labels";
  import GameLogo from "./GameLogo.svelte";

  let {
    game,
    portrait = false,
    showLogo = true,
    artworkAsset = null,
    playtimeMilliseconds = 0,
    lastPlayedAt = null,
    launch,
    favorite = false,
    onToggleFavorite,
    onOrganize,
    onOpen,
    class: className = "",
  }: {
    game: Game;
    portrait?: boolean;
    showLogo?: boolean;
    artworkAsset?: SteamAssetKind | null;
    playtimeMilliseconds?: number;
    lastPlayedAt?: number | null;
    launch: GameLaunchState | undefined;
    favorite?: boolean;
    onToggleFavorite?: () => void;
    onOrganize?: () => void;
    onOpen: () => void;
    class?: string;
  } = $props();

  const steamAppId = $derived(game.steamAppId);
  const showTitle = $derived(showLogo && (!portrait || steamAppId === null));
  const status = $derived(launch?.status ?? "idle");
  const monogram = $derived(game.name.trim().charAt(0).toUpperCase() || "?");
  const lastPlayedLabel = $derived(
    lastPlayedAt === null
      ? null
      : formatDate(lastPlayedAt, $language, { day: "numeric", month: "short" }),
  );

  const playtimeLabel = $derived(formatPlaytime(playtimeMilliseconds));
</script>

<ArtworkTile {steamAppId} {monogram} {portrait} asset={artworkAsset} class={className}>
  {#snippet children(visible)}
    <button
      type="button"
      class="flex flex-1 flex-col text-left focus-visible:-outline-offset-2"
      aria-label={t("Dettagli di {0}", $language, [game.name])}
      onclick={onOpen}
    >
      <div class="absolute inset-x-2 top-2 flex items-center gap-1.5">
        {#if lastPlayedLabel !== null}
          <span
            class="rounded-full bg-zinc-950/60 px-2 py-0.5 text-xs font-medium text-legio-fg light:bg-zinc-100/70"
            title={t("Ultima partita: {0}", $language, [lastPlayedLabel])}
          >
            {lastPlayedLabel}
          </span>
        {/if}
        <div class="ml-auto flex items-center gap-1.5">
          {#if $gameUpdates.data.some(update => update.gameId === game.id || (game.steamAppId !== null && update.steamAppId === game.steamAppId))}<span class="rounded-full bg-legio-download px-2 py-0.5 text-xs font-medium text-white" title={t("Aggiornamento disponibile", $language)}><Icon name="download" size="sm" /></span>{/if}
          {#if status !== "idle"}
            <Badge tone={status === "running" ? "success" : "warning"} title={status === "running" ? t("In esecuzione", $language) : t("Avvio in corso", $language)} />
          {/if}
          <span
            class="flex items-center gap-1 rounded-full bg-zinc-950/60 px-2 py-0.5 text-xs font-medium tabular-nums text-legio-fg light:bg-zinc-100/70"
            title={t("{0} giocati", $language, [playtimeLabel])}
          >
            <Icon name="clock" size="sm" />
            {playtimeLabel}
          </span>
        </div>
      </div>

      {#if showTitle}
        <div class="relative p-3 {portrait ? 'mt-auto' : 'm-auto'}">
          <div class="flex w-full min-w-0 max-w-full flex-col items-center">
            <GameLogo {game} {visible} />
          </div>
        </div>
      {/if}
    </button>
    {#if onToggleFavorite || onOrganize}
      <!-- Siblings of the open button, since buttons cannot nest. Shown on hover or focus, and always for a favorite. -->
      <div class="absolute right-2 bottom-2 flex items-center gap-1.5">
        {#if onOrganize}
          <Button label={t("Organizza {0}", $language, [game.name])} variant="overlay" shape="circle" size="sm" class="opacity-0 group-hover:opacity-100 focus-visible:opacity-100 group-focus-within:opacity-100" onClick={onOrganize}>
            <Icon name="collection" />
          </Button>
        {/if}
        {#if onToggleFavorite}
          <Button
            label={favorite ? t("Rimuovi dai preferiti", $language) : t("Aggiungi ai preferiti", $language)}
            variant="overlay"
            shape="circle"
            size="sm"
            pressed={favorite}
            class={favorite ? "" : "opacity-0 group-hover:opacity-100 focus-visible:opacity-100 group-focus-within:opacity-100"}
            onClick={onToggleFavorite}
          >
            <Icon name="star" class={favorite ? "fill-current text-legio-warning" : ""} />
          </Button>
        {/if}
      </div>
    {/if}
  {/snippet}
</ArtworkTile>

