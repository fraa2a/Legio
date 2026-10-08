<script lang="ts">
  import { t, language } from "../../i18n";
  import type { Game } from "../../services/local-state";
  import type { GameLaunchState } from "../../services/steam-accounts";
  import type { SteamAssetKind } from "../../services/steam-details";
  import ArtworkTile from "../../components/ui/ArtworkTile.svelte";
  import Badge from "../../components/ui/Badge.svelte";
  import Icon from "../../components/ui/Icon.svelte";
  import GameLogo from "./GameLogo.svelte";

  let {
    game,
    portrait = false,
    showLogo = true,
    artworkAsset = null,
    playtimeMilliseconds = 0,
    lastPlayedAt = null,
    launch,
    onOpen,
  }: {
    game: Game;
    portrait?: boolean;
    showLogo?: boolean;
    artworkAsset?: SteamAssetKind | null;
    playtimeMilliseconds?: number;
    lastPlayedAt?: number | null;
    launch: GameLaunchState | undefined;
    onOpen: () => void;
  } = $props();

  const steamAppId = $derived(game.steamAppId);
  const showTitle = $derived(showLogo && (!portrait || steamAppId === null));
  const status = $derived(launch?.status ?? "idle");
  const monogram = $derived(game.name.trim().charAt(0).toUpperCase() || "?");
  const lastPlayedLabel = $derived(
    lastPlayedAt === null
      ? null
      : new Date(lastPlayedAt).toLocaleDateString(undefined, { day: "numeric", month: "short" }),
  );

  const playtimeLabel = $derived.by(() => {
    const hours = Math.floor(playtimeMilliseconds / 3600000);
    if (hours >= 1) return hours === 1 ? "1h" : `${hours}h`;
    return `${Math.floor(playtimeMilliseconds / 60000)}m`;
  });
</script>

<ArtworkTile {steamAppId} {monogram} {portrait} asset={artworkAsset}>
  <button
    type="button"
    class="flex flex-1 flex-col text-left focus-visible:outline-2 focus-visible:outline-offset-[-2px] focus-visible:outline-white"
    aria-label="{t("Dettagli di ", $language)}{game.name}"
    onclick={onOpen}
  >
    <div class="absolute inset-x-2 top-2 flex items-center gap-1.5">
      {#if lastPlayedLabel !== null}
        <span
          class="rounded-full bg-zinc-950/60 px-2 py-0.5 text-xs font-medium text-zinc-100 light:bg-zinc-100/70 light:text-zinc-900"
          title="{t("Ultima partita: ", $language)}{lastPlayedLabel}"
        >
          {lastPlayedLabel}
        </span>
      {/if}
      <div class="ml-auto flex items-center gap-1.5">
        {#if status !== "idle"}
          <Badge tone={status === "running" ? "success" : "warning"} title={status === "running" ? t("In esecuzione", $language) : t("Avvio in corso", $language)} />
        {/if}
        <span
          class="flex items-center gap-1 rounded-full bg-zinc-950/60 px-2 py-0.5 text-xs font-medium tabular-nums text-zinc-100 light:bg-zinc-100/70 light:text-zinc-900"
          title="{playtimeLabel}{t(" giocati", $language)}"
        >
          <Icon name="clock" size="h-3.5 w-3.5" />
          {playtimeLabel}
        </span>
      </div>
    </div>

    {#if showTitle}
      <div class="relative p-3 {portrait ? 'mt-auto' : 'm-auto'}">
        <div class="flex w-full min-w-0 max-w-full flex-col items-center">
          <GameLogo {game} />
        </div>
      </div>
    {/if}
  </button>
</ArtworkTile>

