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
  <!-- One button for every state, so the color transitions instead of switching. -->
  <Button
    label={status === "idle" ? playLabel : status === "launching" ? (cancelPending ? t("ANNULLAMENTO...", $language) : t("AVVIO...", $language)) : t("FERMA", $language)}
    variant={actionVariant}
    {shape}
    {size}
    class="duration-(--legio-motion-slow) {status === 'idle' && revealOnHover ? 'opacity-0 group-hover:opacity-100 group-focus-within:opacity-100' : className}"
    disabled={status === "idle" ? actionPending : status === "launching" ? cancelPending : actionPending}
    onClick={() => (status === "idle" ? onPlay(game) : status === "launching" ? onCancel(game) : onStop(game))}
  >
    <Icon name={status === "idle" ? "play" : status === "launching" ? "close" : "stop"} size={iconSize} />
  </Button>
{/if}
