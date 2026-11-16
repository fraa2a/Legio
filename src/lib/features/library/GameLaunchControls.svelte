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
    shape = "rect",
    size = "md",
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
    shape?: "rect" | "pill";
    size?: "md" | "hero";
    revealOnHover?: boolean;
    class?: string;
    onPlay: (game: Game) => void;
    onCancel: (game: Game) => void;
    onStop: (game: Game) => void;
  } = $props();

  const status = $derived(launch?.status ?? "idle");
  const iconSize = $derived(size === "hero" ? "xl" : "md");
  const actionVariant = $derived(
    status === "launching" ? "launching" : status === "running" ? "running" : variant === "primary" ? "play" : "secondary",
  );
</script>

{#if game.steamInstallPath !== null || game.executablePath !== null}
  {#if status === "idle"}
    <Button
      label={playLabel}
      variant={actionVariant}
      {shape}
      {size}
      class={revealOnHover
        ? "opacity-0 group-hover:opacity-100 group-focus-within:opacity-100"
        : className}
      disabled={actionPending}
      onClick={() => onPlay(game)}
    >
      <Icon name="play" size={iconSize} />
    </Button>
  {:else if status === "launching"}
    <Button
      label={cancelPending ? t("ANNULLAMENTO...", $language) : t("AVVIO...", $language)}
      variant={actionVariant}
      {shape}
      {size}
      class={className}
      disabled={cancelPending}
      onClick={() => onCancel(game)}
    >
      <Icon name="close" size={iconSize} />
    </Button>
  {:else}
    <Button label={t("FERMA", $language)} variant={actionVariant} {shape} {size} class={className} disabled={actionPending} onClick={() => onStop(game)}>
      <Icon name="stop" size={iconSize} />
    </Button>
  {/if}
{/if}
