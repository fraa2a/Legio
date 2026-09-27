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
    circle?: boolean;
    revealOnHover?: boolean;
    class?: string;
    onPlay: (game: Game) => void;
    onCancel: (game: Game) => void;
    onStop: (game: Game) => void;
  } = $props();

  const status = $derived(launch?.status ?? "idle");
</script>

{#if game.steamInstallPath !== null || game.executablePath !== null}
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
          : className}
      >
        <Icon name="play" size="h-5 w-5 translate-x-px" />
      </Button>
    {:else}
      <Button label="Gioca" {variant} class={className} disabled={actionPending} onClick={() => onPlay(game)} />
    {/if}
  {:else if status === "launching"}
    <Button
      label={cancelPending ? "Annullamento..." : "Annulla avvio"}
      {variant}
      class={className}
      disabled={cancelPending}
      onClick={() => onCancel(game)}
    />
  {:else}
    <Button label="Arresta" {variant} class={className} disabled={actionPending} onClick={() => onStop(game)} />
  {/if}
{/if}
