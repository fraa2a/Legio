<script lang="ts">
  import { t, language } from "../../i18n";
  import type { Game } from "../../services/local-state";
  import type { GameLaunchState } from "../../services/steam-accounts";
  import Button from "../../components/ui/Button.svelte";
  import Icon from "../../components/ui/Icon.svelte";
  import { launchProfiles } from "../../stores/launch-profiles";
  import LaunchProfileSplit from "./LaunchProfileSplit.svelte";
  import { lastUsedProfileId, profileMenuOptions, profilesOf } from "./launch-profiles";

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
    menuAbove = false,
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
    menuAbove?: boolean;
    class?: string;
    // A null profile is the game's base settings.
    onPlay: (game: Game, profileId: string | null) => void | Promise<void>;
    onCancel: (game: Game) => void;
    onStop: (game: Game) => void;
  } = $props();

  const status = $derived(launch?.status ?? "idle");
  const iconSize = $derived(size === "hero" ? "xl" : "md");
  const launchingLabel = $derived(
    launch?.steamWait === "signing_in"
      ? t("ACCEDI A STEAM...", $language)
      : launch?.steamWait === "starting"
        ? t("ATTESA DI STEAM...", $language)
        : t("AVVIO...", $language),
  );
  const actionVariant = $derived(
    status === "launching" ? "launching" : status === "running" ? "running" : variant === "primary" ? "play" : "secondary",
  );
  const profiles = $derived(profilesOf($launchProfiles.data, game.id));
  const launchable = $derived(game.steamInstallPath !== null || game.executablePath !== null);

  $effect(() => {
    if (launchable && $launchProfiles.status === "idle") void launchProfiles.load();
  });

  async function play(profileId: string | null): Promise<void> {
    try {
      await onPlay(game, profileId);
    } finally {
      // The backend records the profile of a launch that started.
      await launchProfiles.reload();
    }
  }
</script>

{#if launchable}
  {#if status === "idle" && profiles.length > 0 && !revealOnHover}
    <LaunchProfileSplit
      label={playLabel}
      options={profileMenuOptions(profiles, t("Predefinito", $language))}
      selectedId={lastUsedProfileId($launchProfiles.data, game.id)}
      {variant}
      {shape}
      {size}
      {menuAbove}
      class={className}
      disabled={actionPending}
      onPlay={(profileId) => void play(profileId)}
    />
  {:else}
    <!-- One button for every state, so the color transitions instead of switching. -->
    <Button
      label={status === "idle" ? playLabel : status === "launching" ? (cancelPending ? t("ANNULLAMENTO...", $language) : launchingLabel) : t("FERMA", $language)}
      variant={actionVariant}
      {shape}
      {size}
      class="duration-(--legio-motion-slow) {status === 'idle' && revealOnHover ? 'opacity-0 group-hover:opacity-100 group-focus-within:opacity-100' : className}"
      disabled={status === "idle" ? actionPending : status === "launching" ? cancelPending : actionPending}
      onClick={() => (status === "idle" ? void onPlay(game, null) : status === "launching" ? onCancel(game) : onStop(game))}
    >
      <Icon name={status === "idle" ? "play" : status === "launching" ? "close" : "stop"} size={iconSize} />
    </Button>
  {/if}
{/if}
