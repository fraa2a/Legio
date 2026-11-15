<script lang="ts">
  import type { ThemePreset } from "../../services/appearance";
  import { ambientColors, loadHeroPalette } from "../../services/ambient-palette";
  import { logApplicationEvent } from "../../services/application-log";
  import { ambientSteamAppId } from "../../stores/ambient-game";
  import { toMessage } from "../../utils/errors";
  import CanvasBackground from "./CanvasBackground.svelte";
  import { createAmbientRenderer } from "./ambient-renderer";

  let { opacity, palette, accent, active = true }: {
    opacity: number;
    palette: ThemePreset;
    accent: string;
    active?: boolean;
  } = $props();

  let samples = $state<string[]>([]);

  $effect(() => {
    const steamAppId = $ambientSteamAppId;
    if (steamAppId === null) {
      samples = [];
      return;
    }
    const loading = loadHeroPalette(steamAppId);
    let cancelled = false;
    void loading.promise.then((colors) => {
      if (!cancelled) samples = colors;
    }, (reason: unknown) => {
      if (cancelled) return;
      samples = [];
      logApplicationEvent("warn", "console", `Ambient background artwork unavailable: ${toMessage(reason)}`);
    });
    return () => {
      cancelled = true;
      loading.cancel();
    };
  });

  const config = $derived({
    background: palette.palette.background,
    colors: ambientColors(samples, { ...palette.palette, accent }, palette.scheme),
  });
</script>

<CanvasBackground {opacity} {active} {config} create={createAmbientRenderer} resolution={0.125} maxFps={30} />
