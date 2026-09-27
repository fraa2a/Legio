<script lang="ts">
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
    circle = false,
    revealOnHover = false,
    onPlay,
    onCancel,
    onStop,
  }: {
    game: Game;
    launch: GameLaunchState | undefined;
    actionPending?: boolean;
    cancelPending?: boolean;
    variant?: "primary" | "secondary";
    circle?: boolean;
    revealOnHover?: boolean;
    onPlay: (game: Game) => void;
    onCancel: (game: Game) => void;
    onStop: (game: Game) => void;
  } = $props();

  const status = $derived(launch?.status ?? "idle");
</script>

{#if game.steamAppId !== null}
  {#if status === "idle"}
    {#if circle}
      <Button
        label="Gioca"
        {variant}
        {circle}
        disabled={actionPending}
        onClick={() => onPlay(game)}
        class={revealOnHover
          ? "opacity-0 group-hover:opacity-100 group-focus-within:opacity-100"
          : undefined}
      >
        <Icon name="play" size="h-5 w-5 translate-x-px" />
      </Button>
    {:else}
      <Button label="Gioca" {variant} disabled={actionPending} onClick={() => onPlay(game)} />
    {/if}
  {:else if status === "launching"}
    <Button
      label={cancelPending ? "Annullamento..." : "Annulla avvio"}
      {variant}
      disabled={cancelPending}
      onClick={() => onCancel(game)}
    />
  {:else}
    <Button label="Arresta" {variant} disabled={actionPending} onClick={() => onStop(game)} />
  {/if}
{/if}
