<script lang="ts">
  import { t, language } from "../../i18n";
  import type { Game } from "../../services/local-state";
  import type { GameLaunchState } from "../../services/steam-accounts";
  import Button from "../../components/ui/Button.svelte";
  import Icon from "../../components/ui/Icon.svelte";

  let {
    game,
    launch,
    actionPending = false,
    cancelPending = false,
    variant = "secondary",
    playLabel = t("GIOCA", $language),
    circle = false,
    revealOnHover = false,
    class: className = "",
    onPlay,
    onCancel,
    onStop,
  }: {
    game: Game;
    launch: GameLaunchState | undefined;
    actionPending?: boolean;
    cancelPending?: boolean;
    variant?: "primary" | "secondary";
    playLabel?: string;
    circle?: boolean;
    revealOnHover?: boolean;
    class?: string;
    onPlay: (game: Game) => void;
    onCancel: (game: Game) => void;
    onStop: (game: Game) => void;
  } = $props();

  const status = $derived(launch?.status ?? "idle");
  const actionVariant = $derived(
    status === "launching" ? "launching" : status === "running" ? "running" : variant === "primary" ? "play" : "secondary",
  );
</script>

{#if game.steamInstallPath !== null || game.executablePath !== null}
  {#if status === "idle"}
    <Button
      label={playLabel}
      variant={actionVariant}
      {circle}
      class={revealOnHover
        ? "opacity-0 group-hover:opacity-100 group-focus-within:opacity-100"
        : className}
      disabled={actionPending}
      onClick={() => onPlay(game)}
    >
      <Icon name="play" size="h-6 w-6" />
    </Button>
  {:else if status === "launching"}
    <Button
      label={cancelPending ? "ANNULLAMENTO..." : "LAUNCHING..."}
      variant={actionVariant}
      class={className}
      disabled={cancelPending}
      onClick={() => onCancel(game)}
    >
      <Icon name="close" size="h-6 w-6" />
    </Button>
  {:else}
    <Button label="STOP" variant={actionVariant} class={className} disabled={actionPending} onClick={() => onStop(game)}>
      <Icon name="stop" size="h-6 w-6" />
    </Button>
  {/if}
{/if}
