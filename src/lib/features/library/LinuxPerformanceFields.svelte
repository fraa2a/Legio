<script lang="ts">
  import { t, language } from "../../i18n";
  import type { LinuxPerformance } from "../../services/game-settings";
  import SelectField from "../../components/ui/SelectField.svelte";
  import Toggle from "../../components/ui/Toggle.svelte";

  let {
    value,
    gameModeAvailable,
    mangohudAvailable,
    gamescopeAvailable,
    idPrefix,
    disabled = false,
    onChange,
  }: {
    value: LinuxPerformance;
    gameModeAvailable: boolean;
    mangohudAvailable: boolean;
    gamescopeAvailable: boolean;
    idPrefix: string;
    disabled?: boolean;
    onChange: (value: LinuxPerformance) => void;
  } = $props();

  function setPerformance(changes: Partial<LinuxPerformance>): void {
    onChange({ ...value, ...changes });
  }

  function setGamescopeResolution(resolution: string): void {
    const config = value.gamescope;
    if (config === null) return;
    if (resolution === "native") {
      setPerformance({ gamescope: { ...config, width: null, height: null } });
    } else {
      const [width, height] = resolution.split("x").map(Number);
      if (Number.isInteger(width) && Number.isInteger(height)) setPerformance({ gamescope: { ...config, width, height } });
    }
  }

  function setGamescopeFps(limit: string): void {
    const config = value.gamescope;
    if (config === null) return;
    const fps = limit === "uncapped" ? null : Number(limit);
    if (fps === null || (Number.isInteger(fps) && fps >= 1 && fps <= 360)) setPerformance({ gamescope: { ...config, fps } });
  }
</script>

<div class="flex flex-col gap-3">
  <span class="text-sm text-legio-fg-muted">{t("Prestazioni Linux", $language)}</span>
  <Toggle label="GameMode" checked={value.gameMode}
    disabled={disabled || (!gameModeAvailable && !value.gameMode)}
    onChange={(checked) => setPerformance({ gameMode: checked })} />
  <Toggle label="MangoHud" checked={value.mangohud}
    disabled={disabled || (!mangohudAvailable && !value.mangohud)}
    onChange={(checked) => setPerformance({ mangohud: checked })} />
  <Toggle label="Gamescope" checked={value.gamescope !== null}
    disabled={disabled || (!gamescopeAvailable && value.gamescope === null)}
    onChange={(checked) => setPerformance({ gamescope: checked ? { width: null, height: null, fps: null } : null })} />
  {#if !gameModeAvailable || !gamescopeAvailable}
    <p class="text-xs text-legio-fg-subtle">{t("Installa GameMode o gamescope e riapri queste impostazioni per abilitarli.", $language)}</p>
  {/if}
  {#if !mangohudAvailable}
    <p class="text-xs text-legio-fg-subtle">{t("Installa MangoHud e riapri queste impostazioni per abilitarlo.", $language)}</p>
  {/if}
  {#if value.gamescope !== null}
    {@const config = value.gamescope}
    <div class="grid gap-4 md:grid-cols-2">
      <SelectField id="{idPrefix}-gamescope-resolution" label={t("Risoluzione interna", $language)}
        value={config.width === null ? "native" : `${config.width}x${config.height}`}
        disabled={disabled}
        options={[{ value: "native", label: t("Nativa", $language) }, ...[...new Set(["1280x720", "1600x900", "1920x1080", "2560x1440", "3840x2160", ...(config.width === null ? [] : [`${config.width}x${config.height}`])])].map((resolution) => ({ value: resolution, label: resolution }))]}
        onChange={setGamescopeResolution} />
      <SelectField id="{idPrefix}-gamescope-fps" label={t("Limite FPS", $language)} value={config.fps === null ? "uncapped" : String(config.fps)}
        disabled={disabled}
        options={[{ value: "uncapped", label: t("Senza limite", $language) }, ...[...new Set([30, 40, 60, 90, 120, 144, 165, 240, 360, ...(config.fps === null ? [] : [config.fps])])].map((fps) => ({ value: String(fps), label: String(fps) }))]}
        onChange={setGamescopeFps} />
    </div>
    <p class="text-xs text-legio-fg-subtle">{t("Una risoluzione inferiore riduce la qualità visiva. Il limite FPS può ridurre consumi e temperature.", $language)}</p>
  {/if}
  <p class="text-xs text-legio-fg-subtle">{t("GameMode e Gamescope sono facoltativi. Confronta prestazioni e frametime sul gioco; Wayland resta una scelta separata.", $language)}</p>
</div>
