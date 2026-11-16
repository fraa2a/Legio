<script lang="ts">
  import { get } from "svelte/store";
  import { createSaveQueue } from "../../services/save-queue";
  import { onDestroy, untrack } from "svelte";
  import { t, language } from "../../i18n";
  import type { Theme } from "../../services/local-state";
  import {
    activePalette, chooseBackground, textScaleRange, cleanupBackgrounds, defaultAppearance, createThemeCode, defaultDither, exportAppearance, importAppearance, readThemeCode, panelStyleOf, panelStyles, themePresets,
    type Appearance, type AppearanceProfile, type Palette, type PanelStyle,
  } from "../../services/appearance";
  import { appearancePreview } from "../../stores/appearance";
  import { appInfo } from "../../stores/app-info";
  import { settings, updateSettings } from "../../stores/settings";
  import Icon from "../../components/ui/Icon.svelte";
  import RadioCard from "../../components/ui/RadioCard.svelte";
  import { toMessage } from "../../utils/errors";
  import Button from "../../components/ui/Button.svelte";
  import ConfirmDialog from "../../components/ui/ConfirmDialog.svelte";
  import TextField from "../../components/ui/TextField.svelte";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import SettingsGroup from "../../components/ui/SettingsGroup.svelte";
  import SegmentedControl from "../../components/ui/SegmentedControl.svelte";
  import SelectField from "../../components/ui/SelectField.svelte";
  import Toggle from "../../components/ui/Toggle.svelte";
  import ResetSetting from "../../components/ui/ResetSetting.svelte";
  import AppearanceSlider from "./AppearanceSlider.svelte";
  import { showToast } from "../../stores/toast";

  type AppearanceDraft = { theme: Theme; appearance: Appearance };
  let { accent, preview, onPreview, onBusyChange }: {
    accent: string;
    preview?: AppearanceDraft;
    onPreview?: (value: AppearanceDraft) => void;
    onBusyChange?: (busy: boolean) => void;
  } = $props();

  type SliderKey = "backgroundBlur" | "backgroundOpacity" | "animatedOpacity" | "surfaceOpacity" | "surfaceBlur" | "dialogOpacity" | "dialogBlur";
  type SliderConfig = { key: SliderKey; label: string; min: number; max: number; unit: string };
  type DitherSliderKey = "waveSpeed" | "waveFrequency" | "waveAmplitude" | "colorNum" | "pixelSize" | "mouseRadius";
  type DitherSliderConfig = { key: DitherSliderKey; label: string; min: number; max: number; step: number; decimals: number };
  type DitherToggleKey = "disableAnimation" | "enableMouseInteraction";

  const lightPalette = themePresets.find((preset) => preset.id === "light");
  const darkPalette = themePresets.find((preset) => preset.id === "dark");
  const initial = untrack(() => preview ?? get(settings).data);
  let theme: Theme = $state(initial.theme);
  let draft: Appearance = $state(structuredClone(initial.appearance));
  let baseline = $state(JSON.stringify({ theme: initial.theme, appearance: initial.appearance }));
  let failedSnapshot: string | null = null;
  const saveQueue = createSaveQueue();
  let saving = $state(false);
  let adjusting = $state(false);
  let busy = $state(false);
  let error = $state<string | null>(null);
  let customPanels = $state(false);
  let pasteOpen = $state(false);
  let pasteText = $state("");
  let pasteError = $state<string | null>(null);
  let disposed = false;

  const serialized = $derived(JSON.stringify({ theme, appearance: draft }));
  const paletteAccent = $derived(activePalette(theme, document.documentElement.dataset.theme !== "light").palette.accent);
  const dirty = $derived(serialized !== baseline);
  const hyprland = $derived($appInfo.data.platform === "linux" && $appInfo.data.desktopEnvironment === "hyprland");
  const themeOptions: { id: Theme; label: string; preset: (typeof themePresets)[number] | null }[] = $derived([
    { id: "system", label: t("Sistema", $language), preset: null },
    ...themePresets.map((preset) => ({ id: preset.id as Theme, label: t(preset.name, $language), preset })),
  ]);
  const backgroundEffects: SliderConfig[] = $derived([
    { key: "backgroundBlur", label: t("Blur dello sfondo", $language), min: 0, max: 40, unit: "px" },
    { key: "backgroundOpacity", label: t("Opacità dell'immagine", $language), min: 0, max: 100, unit: "%" },
  ]);
  const surfaceEffects: SliderConfig[] = $derived([
    { key: "surfaceOpacity", label: t("Opacità dei pannelli", $language), min: 0, max: 100, unit: "%" },
    { key: "surfaceBlur", label: t("Blur dei pannelli", $language), min: 0, max: 40, unit: "px" },
    { key: "dialogOpacity", label: t("Opacità dei dialoghi", $language), min: 0, max: 100, unit: "%" },
    { key: "dialogBlur", label: t("Blur dei dialoghi", $language), min: 0, max: 40, unit: "px" },
  ]);
  const animatedOptions: { id: Appearance["animatedBackground"]; label: string }[] = $derived([
    { id: "none", label: t("Nessuno", $language) },
    { id: "particles", label: t("Punti connessi", $language) },
    { id: "dither", label: "Dither" },
    { id: "scanlines", label: t("Linee di scansione", $language) },
    { id: "aurora", label: "Aurora" },
    { id: "ambient", label: t("Ambientale", $language) },
    { id: "starfield", label: t("Campo stellare", $language) },
    { id: "seasonal", label: t("Stagionale", $language) },
  ]);
  const seasonOptions = $derived([
    { value: "snow", label: t("Neve", $language) },
    { value: "rain", label: t("Pioggia", $language) },
    { value: "leaves", label: t("Foglie", $language) },
  ]);
  const starDensityOptions = $derived([
    { value: "low", label: t("Bassa", $language) },
    { value: "medium", label: t("Media", $language) },
    { value: "high", label: t("Alta", $language) },
  ]);
  const panelStyleOptions: { value: PanelStyle | "custom"; label: string }[] = $derived([
    { value: "flat", label: t("Piatto", $language) },
    { value: "glass", label: t("Vetro", $language) },
    { value: "solid", label: t("Solido", $language) },
    { value: "custom", label: t("Personalizzato", $language) },
  ]);
  const densityOptions = $derived([
    { value: "compact", label: t("Compatta", $language) },
    { value: "normal", label: t("Normale", $language) },
    { value: "wide", label: t("Ampia", $language) },
  ]);
  const cornerOptions = $derived([
    { value: "sharp", label: t("Squadrati", $language) },
    { value: "soft", label: t("Morbidi", $language) },
    { value: "round", label: t("Tondi", $language) },
  ]);
  const panelStyle = $derived(customPanels ? "custom" : (panelStyleOf(draft) ?? "custom"));
  const ditherEffects: DitherSliderConfig[] = $derived([
    { key: "waveSpeed", label: t("Velocità dell'onda", $language), min: 0, max: 1, step: 0.01, decimals: 2 },
    { key: "waveFrequency", label: t("Frequenza dell'onda", $language), min: 0, max: 10, step: 0.1, decimals: 1 },
    { key: "waveAmplitude", label: t("Ampiezza dell'onda", $language), min: 0, max: 1, step: 0.01, decimals: 2 },
    { key: "colorNum", label: t("Numero di colori", $language), min: 2, max: 64, step: 1, decimals: 0 },
    { key: "pixelSize", label: t("Dimensione dei pixel", $language), min: 1, max: 16, step: 1, decimals: 0 },
    { key: "mouseRadius", label: t("Raggio del mouse", $language), min: 0, max: 2, step: 0.05, decimals: 2 },
  ]);
  const ditherToggles: { key: DitherToggleKey; label: string }[] = $derived([
    { key: "disableAnimation", label: t("Disattiva animazione", $language) },
    { key: "enableMouseInteraction", label: t("Interazione con il mouse", $language) },
  ]);

  $effect(() => {
    const value = { theme, appearance: structuredClone($state.snapshot(draft)) };
    if (onPreview) onPreview(value);
    else appearancePreview.set(value);
  });

  $effect(() => { onBusyChange?.(busy); });

  $effect(() => {
    if (onPreview || !dirty || busy || saving || adjusting || serialized === failedSnapshot) return;
    const timer = setTimeout(() => void save(), 500);
    return () => clearTimeout(timer);
  });

  onDestroy(() => {
    disposed = true;
    onBusyChange?.(false);
    if (onPreview) return;
    appearancePreview.set(null);
    void saveQueue.flush(async () => {
      if (!busy && dirty && serialized !== failedSnapshot) await save();
      await cleanupBackgrounds();
    }).catch((reason) => console.error("Could not finish saving appearance", reason));
  });

  function save(): Promise<void> {
    const snapshot = serialized;
    const selectedTheme = theme;
    const selectedAppearance = structuredClone($state.snapshot(draft));
    return saveQueue.run(async () => {
      saving = true;
      error = null;
      try {
        await updateSettings({ theme: selectedTheme, appearance: selectedAppearance });
        baseline = snapshot;
        failedSnapshot = null;
        if (!disposed && serialized === snapshot) showToast(t("Aspetto salvato.", get(language)), "success");
        await cleanupBackgrounds();
      } catch (reason) {
        failedSnapshot = snapshot;
        error = toMessage(reason);
      } finally {
        saving = false;
      }
    });
  }

  async function selectBackground(): Promise<void> {
    busy = true;
    error = null;
    try {
      const id = await chooseBackground();
      if (!disposed && id !== null) draft.background = id;
      if (disposed) await cleanupBackgrounds();
    } catch (reason) {
      error = toMessage(reason);
    } finally {
      busy = false;
    }
  }

  function setDitherValue(key: DitherSliderKey, value: number, decimals: number): void {
    draft.dither[key] = Math.round(value * 10 ** decimals) / 10 ** decimals;
  }

  function reset(): void {
    theme = "system";
    draft = defaultAppearance();
    customPanels = false;
  }

  function selectPanelStyle(value: string): void {
    customPanels = value === "custom";
    if (value !== "custom") Object.assign(draft, panelStyles[value as PanelStyle]);
  }

  function setPanelValue(key: SliderKey, value: number): void {
    draft[key] = value;
    customPanels = panelStyleOf(draft) === null;
  }

  async function exportProfile(): Promise<void> {
    busy = true;
    error = null;
    try {
      if (await exportAppearance(theme, $state.snapshot(draft))) showToast(t("Aspetto esportato.", get(language)), "success");
    } catch (reason) {
      error = toMessage(reason);
    } finally {
      busy = false;
    }
  }

  function applyProfile(profile: AppearanceProfile): void {
    theme = profile.theme;
    draft = { ...profile.appearance, background: draft.background };
    customPanels = panelStyleOf(draft) === null;
  }

  async function importProfile(): Promise<void> {
    busy = true;
    error = null;
    try {
      const profile = await importAppearance();
      if (profile === null || disposed) return;
      applyProfile(profile);
    } catch (reason) {
      error = toMessage(reason);
    } finally {
      busy = false;
    }
  }

  async function copyThemeCode(asLink: boolean): Promise<void> {
    busy = true;
    error = null;
    try {
      const code = await createThemeCode(theme, $state.snapshot(draft));
      await navigator.clipboard.writeText(asLink ? "legio://theme?code=" + code : code);
      showToast(asLink ? t("Collegamento copiato.", get(language)) : t("Codice copiato.", get(language)), "success");
    } catch (reason) {
      error = toMessage(reason);
    } finally {
      busy = false;
    }
  }

  async function applyPastedCode(): Promise<void> {
    busy = true;
    pasteError = null;
    try {
      const profile = await readThemeCode(pasteText);
      if (disposed) return;
      applyProfile(profile);
      pasteOpen = false;
      pasteText = "";
    } catch (reason) {
      pasteError = toMessage(reason);
    } finally {
      busy = false;
    }
  }
</script>

<svelte:window
  onpointerdown={(event) => {
    if (event.target instanceof HTMLInputElement && event.target.type === "range" && event.target.closest(".appearance-settings")) adjusting = true;
  }}
  onpointerup={() => { adjusting = false; }}
  onpointercancel={() => { adjusting = false; }}
/>

<section class="appearance-settings flex flex-col gap-6">
  {#if error}<ErrorBanner message={error} />{/if}

  <SettingsGroup title={t("Tema", $language)} icon="palette" collapsible defaultOpen>
    <fieldset disabled={busy} class="grid grid-cols-2 gap-3 sm:grid-cols-3 lg:grid-cols-4">
      <legend class="sr-only">{t("Palette predefinite", $language)}</legend>
      {#each themeOptions as option (option.id)}
        <RadioCard variant="tile" name="theme" value={option.id} checked={theme === option.id} onSelect={() => { theme = option.id; }}>
          <span class="relative block h-20 overflow-hidden rounded-control border border-legio-line-subtle" aria-hidden="true">
            {#if option.preset}
              {@render paletteMock(option.preset.palette, "inset-0")}
            {:else if darkPalette && lightPalette}
              <span class="absolute inset-y-0 left-0 w-1/2 overflow-hidden">{@render paletteMock(darkPalette.palette, "inset-y-0 left-0 w-[200%]")}</span>
              <span class="absolute inset-y-0 right-0 w-1/2 overflow-hidden">{@render paletteMock(lightPalette.palette, "inset-y-0 right-0 w-[200%]")}</span>
            {/if}
          </span>
          <span class="flex items-center justify-between gap-2 px-1">
            <span class="truncate text-sm text-legio-fg">{option.label}</span>
            {#if theme === option.id}<span class="shrink-0 text-legio-accent"><Icon name="check" /></span>{/if}
          </span>
        </RadioCard>
      {/each}
    </fieldset>
    <div class="flex items-center gap-1">
      <label for="theme-accent" class="flex min-w-0 flex-1 items-center justify-between gap-2 text-sm text-legio-fg"><span>{t("Colore di accento", $language)}</span><input id="theme-accent" type="color" disabled={busy} value={draft.accent ?? paletteAccent} class="h-control-md w-12 cursor-pointer rounded-control border border-legio-line bg-transparent" oninput={(event) => { draft.accent = event.currentTarget.value; }} /></label>
      <ResetSetting label={t("Ripristina {0}", $language, [t("Colore di accento", $language)])} disabled={busy || draft.accent === null} onClick={() => { draft.accent = null; }} />
    </div>
    <div class="flex items-center gap-2"><div class="min-w-0 flex-1"><Toggle label={t("Accento dalla copertina del gioco", $language)} checked={draft.gameAccent} disabled={busy} onChange={(checked) => { draft.gameAccent = checked; }} /></div><ResetSetting label={t("Ripristina {0}", $language, [t("Accento dalla copertina del gioco", $language)])} disabled={busy || !draft.gameAccent} onClick={() => { draft.gameAccent = false; }} /></div>
  </SettingsGroup>

  <SettingsGroup title={t("Sfondo e animazioni", $language)} icon="image" collapsible>
    <div class="flex flex-wrap gap-2">
      <Button variant="secondary" label={t("Scegli uno sfondo", $language)} disabled={busy || saving} onClick={() => void selectBackground()} />
      {#if draft.background !== null}<Button variant="danger-soft" label={t("Rimuovi sfondo", $language)} disabled={busy} onClick={() => { draft.background = null; }} />{/if}
    </div>
    {#if draft.background !== null}
      <p class="text-xs text-legio-fg-subtle">{t("PNG, JPEG o WebP, fino a 16 MiB e 16 megapixel.", $language)}</p>
      <div class="grid gap-3 sm:grid-cols-2">
        {#each backgroundEffects as effect (effect.key)}
          <AppearanceSlider id={"theme-" + effect.key} label={effect.label} value={draft[effect.key]} min={effect.min} max={effect.max} unit={effect.unit} resetValue={defaultAppearance()[effect.key]} disabled={busy} onChange={(value) => { draft[effect.key] = value; }} />
        {/each}
      </div>
    {/if}
    <SelectField id="theme-animated-background" label={t("Effetto di sfondo animato", $language)} options={animatedOptions.map((option) => ({ value: option.id, label: option.label }))} value={draft.animatedBackground} disabled={busy} onChange={(value) => { draft.animatedBackground = value as Appearance["animatedBackground"]; }} />
    {#if draft.animatedBackground !== "none"}
      <ResetSetting label={t("Ripristina sfondo animato", $language)} disabled={busy} onClick={() => { draft.animatedBackground = "none"; }} />
      <AppearanceSlider id="theme-animatedOpacity" label={t("Opacità dello sfondo animato", $language)} value={draft.animatedOpacity} min={0} max={100} unit="%" resetValue={defaultAppearance().animatedOpacity} disabled={busy} onChange={(value) => { draft.animatedOpacity = value; }} />
    {/if}
    {#if draft.animatedBackground === "ambient"}
      <p class="text-xs text-legio-fg-subtle">{t("Prende i colori dalla copertina del gioco aperto o, nella Home, dell'ultimo giocato.", $language)}</p>
    {:else if draft.animatedBackground === "starfield"}
      <SegmentedControl name="star-density" label={t("Densità delle stelle", $language)} options={starDensityOptions} value={draft.starDensity} disabled={busy} onChange={(value) => { draft.starDensity = value as Appearance["starDensity"]; }} />
    {:else if draft.animatedBackground === "seasonal"}
      <SegmentedControl name="season" label={t("Stagione", $language)} options={seasonOptions} value={draft.season} disabled={busy} onChange={(value) => { draft.season = value as Appearance["season"]; }} />
    {/if}
    <AppearanceSlider id="theme-grain" label={t("Grana", $language)} value={draft.grain} min={0} max={100} unit="%" resetValue={defaultAppearance().grain} disabled={busy} onChange={(value) => { draft.grain = value; }} />
    {#if draft.animatedBackground === "dither" || draft.animatedBackground === "scanlines"}
      <fieldset disabled={busy} class="grid gap-3 sm:grid-cols-2">
        <legend class="mb-2 text-sm font-medium text-legio-fg">{t("Impostazioni dither", $language)}</legend>
        {#each ditherEffects as effect (effect.key)}
          <AppearanceSlider id={"theme-dither-" + effect.key} label={effect.label} value={draft.dither[effect.key]} min={effect.min} max={effect.max} step={effect.step} unit="" resetValue={defaultDither()[effect.key]} disabled={busy} onChange={(value) => setDitherValue(effect.key, value, effect.decimals)} />
        {/each}
        <div class="flex items-center gap-1">
          <label for="theme-dither-waveColor" class="flex min-w-0 flex-1 items-center justify-between gap-2 text-sm text-legio-fg"><span>{t("Colore dell'onda", $language)}</span><input id="theme-dither-waveColor" type="color" value={draft.dither.waveColor ?? accent} class="h-control-md w-12 cursor-pointer rounded-control border border-legio-line bg-transparent" oninput={(event) => { draft.dither.waveColor = event.currentTarget.value; }} /></label>
          <ResetSetting label={t("Ripristina {0}", $language, [t("Colore dell'onda", $language)])} disabled={busy || draft.dither.waveColor === null} onClick={() => { draft.dither.waveColor = null; }} />
        </div>
        <div class="flex items-center gap-1">
          <label for="theme-dither-backgroundColor" class="flex min-w-0 flex-1 items-center justify-between gap-2 text-sm text-legio-fg"><span>{t("Colore di fondo", $language)}</span><input id="theme-dither-backgroundColor" type="color" value={draft.dither.backgroundColor} class="h-control-md w-12 cursor-pointer rounded-control border border-legio-line bg-transparent" oninput={(event) => { draft.dither.backgroundColor = event.currentTarget.value; }} /></label>
          <ResetSetting label={t("Ripristina {0}", $language, [t("Colore di fondo", $language)])} disabled={busy || draft.dither.backgroundColor === defaultDither().backgroundColor} onClick={() => { draft.dither.backgroundColor = defaultDither().backgroundColor; }} />
        </div>
        {#each ditherToggles as toggle (toggle.key)}
          <div class="flex items-center gap-2"><div class="min-w-0 flex-1"><Toggle label={toggle.label} checked={draft.dither[toggle.key]} disabled={busy} onChange={(checked) => { draft.dither[toggle.key] = checked; }} /></div><ResetSetting label={t("Ripristina {0}", $language, [toggle.label])} disabled={busy || draft.dither[toggle.key] === defaultDither()[toggle.key]} onClick={() => { draft.dither[toggle.key] = defaultDither()[toggle.key]; }} /></div>
        {/each}
      </fieldset>
    {/if}
  </SettingsGroup>

  <SettingsGroup title={t("Pannelli e dialoghi", $language)} icon="panels" collapsible>
    <SegmentedControl name="panel-style" label={t("Stile dei pannelli", $language)} options={panelStyleOptions} value={panelStyle} disabled={busy} onChange={selectPanelStyle} />
    <div class="grid gap-3 sm:grid-cols-2">
      {#each surfaceEffects as effect (effect.key)}
        <AppearanceSlider id={"theme-" + effect.key} label={effect.label} value={draft[effect.key]} min={effect.min} max={effect.max} unit={effect.unit} resetValue={defaultAppearance()[effect.key]} disabled={busy} onChange={(value) => setPanelValue(effect.key, value)} />
      {/each}
    </div>
  </SettingsGroup>

  <SettingsGroup title={t("Interfaccia", $language)} icon="sliders" collapsible>
    <SegmentedControl name="density" label={t("Densità", $language)} options={densityOptions} value={draft.density} disabled={busy} onChange={(value) => { draft.density = value as Appearance["density"]; }} />
    <SegmentedControl name="corners" label={t("Angoli", $language)} options={cornerOptions} value={draft.corners} disabled={busy} onChange={(value) => { draft.corners = value as Appearance["corners"]; }} />
    <AppearanceSlider id="theme-textScale" label={t("Dimensione del testo", $language)} value={draft.textScale} min={textScaleRange.min} max={textScaleRange.max} step={5} unit="%" resetValue={defaultAppearance().textScale} disabled={busy} onChange={(value) => { draft.textScale = value; }} />
  </SettingsGroup>

  {#if hyprland}
    <SettingsGroup title={t("Trasparenza della finestra", $language)} icon="window" collapsible>
      <div class="flex items-center gap-2"><div class="min-w-0 flex-1"><Toggle label={t("Trasparenza della finestra su Hyprland", $language)} checked={draft.transparent} disabled={busy} onChange={(checked) => { draft.transparent = checked; }} /></div><ResetSetting label={t("Ripristina trasparenza", $language)} disabled={busy || !draft.transparent} onClick={() => { draft.transparent = false; }} /></div>
    </SettingsGroup>
  {/if}

  <div class="flex flex-wrap items-center justify-between gap-3 pt-4">
    <Button variant="secondary" label={t("Ripristina aspetto predefinito", $language)} disabled={busy} onClick={reset} />
    <div class="flex flex-wrap gap-2">
      <Button variant="secondary" label={t("Copia codice", $language)} disabled={busy} onClick={() => void copyThemeCode(false)} />
      <Button variant="secondary" label={t("Copia collegamento", $language)} disabled={busy} onClick={() => void copyThemeCode(true)} />
      <Button variant="secondary" label={t("Incolla codice", $language)} disabled={busy} onClick={() => { pasteError = null; pasteOpen = true; }} />
      <Button variant="secondary" label={t("Importa aspetto", $language)} disabled={busy} onClick={() => void importProfile()} />
      <Button variant="secondary" label={t("Esporta aspetto", $language)} disabled={busy} onClick={() => void exportProfile()} />
    </div>
  </div>
</section>

{#if pasteOpen}
  <ConfirmDialog
    title={t("Incolla codice tema", $language)}
    message={t("Incolla un codice tema. Lo sfondo resta quello attuale.", $language)}
    confirmLabel={t("Applica", $language)}
    pending={busy}
    confirmDisabled={pasteText.trim() === ""}
    error={pasteError}
    onConfirm={() => void applyPastedCode()}
    onClose={() => { pasteOpen = false; }}
  >
    <TextField id="theme-paste-code" type="textarea" rows={4} monospace label={t("Codice tema", $language)} bind:value={pasteText} />
  </ConfirmDialog>
{/if}

{#snippet paletteMock(palette: Palette, frame: string)}
  <span class="absolute {frame}" style:background-color={palette.background}>
    <span class="absolute inset-y-0 left-0 w-1/5" style:background-color={palette.surface}></span>
    <span class="absolute top-2.5 right-3 left-[28%] h-1.5 rounded-full opacity-70" style:background-color={palette.text}></span>
    <span class="absolute top-6 left-[28%] h-3 w-[22%] rounded-full" style:background-color={palette.raised}></span>
    <span class="absolute top-6 left-[54%] h-3 w-[22%] rounded-full" style:background-color={palette.raised}></span>
    <span class="absolute bottom-2.5 left-[28%] h-2.5 w-[26%] rounded-full" style:background-color={palette.accent}></span>
  </span>
{/snippet}

<style>
  .appearance-settings { min-width: 0; }
</style>
