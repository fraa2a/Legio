<script lang="ts">
  import { get } from "svelte/store";
  import { onDestroy } from "svelte";
  import { t, language } from "../../i18n";
  import type { Theme } from "../../services/local-state";
  import { saveSettings } from "../../services/local-state";
  import {
    chooseBackground, cleanupBackgrounds, defaultAppearance, defaultDither, themePresets,
    type Appearance,
  } from "../../services/appearance";
  import { appearancePreview } from "../../stores/appearance";
  import { appInfo } from "../../stores/app-info";
  import { settings } from "../../stores/settings";
  import { toMessage } from "../../utils/errors";
  import Button from "../../components/ui/Button.svelte";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import SettingsGroup from "../../components/ui/SettingsGroup.svelte";
  import Toggle from "../../components/ui/Toggle.svelte";
  import ResetSetting from "../../components/ui/ResetSetting.svelte";
  import AppearanceSlider from "./AppearanceSlider.svelte";
  import { showToast } from "../../stores/toast";

  let { accent }: { accent: string } = $props();

  type SliderKey = "backgroundBlur" | "backgroundOpacity" | "animatedOpacity" | "surfaceOpacity" | "surfaceBlur" | "dialogOpacity" | "dialogBlur";
  type SliderConfig = { key: SliderKey; label: string; min: number; max: number; unit: string };
  type DitherSliderKey = "waveSpeed" | "waveFrequency" | "waveAmplitude" | "colorNum" | "pixelSize" | "mouseRadius";
  type DitherSliderConfig = { key: DitherSliderKey; label: string; min: number; max: number; step: number; decimals: number };
  type DitherToggleKey = "disableAnimation" | "enableMouseInteraction";

  const initial = get(settings).data;
  let theme: Theme = $state(initial.theme);
  let draft: Appearance = $state(structuredClone(initial.appearance));
  let baseline = $state(JSON.stringify({ theme: initial.theme, appearance: initial.appearance }));
  let failedSnapshot: string | null = null;
  let savingTask: Promise<void> | null = null;
  let saving = $state(false);
  let adjusting = $state(false);
  let busy = $state(false);
  let error = $state<string | null>(null);
  let disposed = false;

  const serialized = $derived(JSON.stringify({ theme, appearance: draft }));
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
  ]);
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
    appearancePreview.set({ theme, appearance: structuredClone($state.snapshot(draft)) });
  });

  $effect(() => {
    if (!dirty || busy || saving || adjusting || serialized === failedSnapshot) return;
    const timer = setTimeout(() => void save(), 500);
    return () => clearTimeout(timer);
  });

  onDestroy(() => {
    disposed = true;
    appearancePreview.set(null);
    void (async () => {
      if (savingTask !== null) await savingTask;
      if (!busy && dirty && serialized !== failedSnapshot) await save();
      await cleanupBackgrounds();
    })().catch((reason) => console.error("Could not finish saving appearance", reason));
  });

  function save(): Promise<void> {
    if (savingTask !== null) return savingTask;
    const snapshot = serialized;
    const selectedTheme = theme;
    const selectedAppearance = structuredClone($state.snapshot(draft));
    savingTask = (async () => {
      saving = true;
      error = null;
      try {
        const saved = await saveSettings({ ...get(settings).data, theme: selectedTheme, appearance: selectedAppearance });
        settings.set(saved);
        baseline = snapshot;
        failedSnapshot = null;
        if (!disposed && serialized === snapshot) showToast(t("Aspetto salvato.", get(language)), "success");
        await cleanupBackgrounds();
      } catch (reason) {
        failedSnapshot = snapshot;
        error = toMessage(reason);
      } finally {
        saving = false;
        savingTask = null;
      }
    })();
    return savingTask;
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
  }
</script>

<svelte:window
  onpointerdown={(event) => {
    if (event.target instanceof HTMLInputElement && event.target.type === "range" && event.target.closest(".appearance-settings")) adjusting = true;
  }}
  onpointerup={() => { adjusting = false; }}
  onpointercancel={() => { adjusting = false; }}
/>

<section class="appearance-settings flex flex-col gap-4">
  {#if error}<ErrorBanner message={error} />{/if}

  <SettingsGroup title={t("Tema", $language)} icon="palette" collapsible>
    <fieldset disabled={busy} class="grid grid-cols-2 gap-2 sm:grid-cols-3">
      <legend class="sr-only">{t("Palette predefinite", $language)}</legend>
      {#each themeOptions as option (option.id)}
        <label class="flex min-h-16 cursor-pointer items-center gap-2 rounded-lg px-3 py-2 transition-colors focus-within:outline-2 focus-within:outline-legio-accent {theme === option.id ? 'bg-legio-accent/10' : 'hover:bg-white/5 light:hover:bg-zinc-900/5'}">
          <input class="sr-only" type="radio" name="theme" value={option.id} checked={theme === option.id} onchange={() => { theme = option.id; }} />
          {#if option.preset}
            <span class="flex shrink-0 -space-x-1" aria-hidden="true"><span class="size-4 rounded-full border border-white/20" style:background-color={option.preset.palette.background}></span><span class="size-4 rounded-full border border-white/20" style:background-color={option.preset.palette.accent}></span></span>
          {:else}
            <span class="grid size-4 shrink-0 place-items-center rounded-full border-2 {theme === option.id ? 'border-legio-accent' : 'border-zinc-500'}" aria-hidden="true">{#if theme === option.id}<span class="size-2 rounded-full bg-legio-accent"></span>{/if}</span>
          {/if}
          <span class="truncate text-sm text-zinc-100 light:text-zinc-900">{option.label}</span>
        </label>
      {/each}
    </fieldset>
  </SettingsGroup>

  <SettingsGroup title={t("Sfondo e animazioni", $language)} icon="image" collapsible>
    <div class="flex flex-wrap gap-2">
      <Button variant="secondary" label={t("Scegli uno sfondo", $language)} disabled={busy || saving} onClick={() => void selectBackground()} />
      {#if draft.background !== null}<Button variant="secondary" label={t("Rimuovi sfondo", $language)} disabled={busy} onClick={() => { draft.background = null; }} />{/if}
    </div>
    {#if draft.background !== null}
      <p class="text-xs text-zinc-500">{t("PNG, JPEG o WebP, fino a 16 MiB e 16 megapixel.", $language)}</p>
      <div class="grid gap-3 sm:grid-cols-2">
        {#each backgroundEffects as effect (effect.key)}
          <AppearanceSlider id={"theme-" + effect.key} label={effect.label} value={draft[effect.key]} min={effect.min} max={effect.max} unit={effect.unit} resetValue={defaultAppearance()[effect.key]} disabled={busy} onChange={(value) => { draft[effect.key] = value; }} />
        {/each}
      </div>
    {/if}
    <fieldset disabled={busy} class="grid gap-2 sm:grid-cols-3">
      <legend class="mb-2 text-sm font-medium text-zinc-100 light:text-zinc-900">{t("Effetto di sfondo animato", $language)}</legend>
      {#each animatedOptions as option (option.id)}
        <label class="flex cursor-pointer items-center gap-2 rounded-xl p-3 text-sm transition-colors focus-within:outline-2 focus-within:outline-offset-2 focus-within:outline-legio-accent {draft.animatedBackground === option.id ? 'bg-legio-accent/10 text-zinc-100 light:text-zinc-900' : 'text-zinc-400 hover:bg-white/5 light:text-zinc-600'}">
          <input class="sr-only" type="radio" name="animated-background" value={option.id} checked={draft.animatedBackground === option.id} onchange={() => { draft.animatedBackground = option.id; }} />
          <span class="grid size-4 shrink-0 place-items-center rounded-full border-2 {draft.animatedBackground === option.id ? 'border-legio-accent' : 'border-zinc-500'}" aria-hidden="true">{#if draft.animatedBackground === option.id}<span class="size-2 rounded-full bg-legio-accent"></span>{/if}</span>
          {option.label}
        </label>
      {/each}
    </fieldset>
    {#if draft.animatedBackground !== "none"}
      <ResetSetting label={t("Ripristina sfondo animato", $language)} disabled={busy} onClick={() => { draft.animatedBackground = "none"; }} />
      <AppearanceSlider id="theme-animatedOpacity" label={t("Opacità dello sfondo animato", $language)} value={draft.animatedOpacity} min={0} max={100} unit="%" resetValue={defaultAppearance().animatedOpacity} disabled={busy} onChange={(value) => { draft.animatedOpacity = value; }} />
    {/if}
    {#if draft.animatedBackground === "dither"}
      <fieldset disabled={busy} class="grid gap-3 sm:grid-cols-2">
        <legend class="mb-2 text-sm font-medium text-zinc-100 light:text-zinc-900">{t("Impostazioni dither", $language)}</legend>
        {#each ditherEffects as effect (effect.key)}
          <AppearanceSlider id={"theme-dither-" + effect.key} label={effect.label} value={draft.dither[effect.key]} min={effect.min} max={effect.max} step={effect.step} unit="" resetValue={defaultDither()[effect.key]} disabled={busy} onChange={(value) => setDitherValue(effect.key, value, effect.decimals)} />
        {/each}
        <div class="flex items-center gap-1">
          <label for="theme-dither-waveColor" class="flex min-w-0 flex-1 items-center justify-between gap-2 text-sm text-zinc-300 light:text-zinc-700"><span>{t("Colore dell'onda", $language)}</span><input id="theme-dither-waveColor" type="color" value={draft.dither.waveColor ?? accent} class="h-9 w-12 cursor-pointer rounded border border-white/15 bg-transparent light:border-zinc-900/15" oninput={(event) => { draft.dither.waveColor = event.currentTarget.value; }} /></label>
          <ResetSetting label={t("Ripristina {0}", $language, [t("Colore dell'onda", $language)])} disabled={busy || draft.dither.waveColor === null} onClick={() => { draft.dither.waveColor = null; }} />
        </div>
        <div class="flex items-center gap-1">
          <label for="theme-dither-backgroundColor" class="flex min-w-0 flex-1 items-center justify-between gap-2 text-sm text-zinc-300 light:text-zinc-700"><span>{t("Colore di fondo", $language)}</span><input id="theme-dither-backgroundColor" type="color" value={draft.dither.backgroundColor} class="h-9 w-12 cursor-pointer rounded border border-white/15 bg-transparent light:border-zinc-900/15" oninput={(event) => { draft.dither.backgroundColor = event.currentTarget.value; }} /></label>
          <ResetSetting label={t("Ripristina {0}", $language, [t("Colore di fondo", $language)])} disabled={busy || draft.dither.backgroundColor === defaultDither().backgroundColor} onClick={() => { draft.dither.backgroundColor = defaultDither().backgroundColor; }} />
        </div>
        {#each ditherToggles as toggle (toggle.key)}
          <div class="flex items-center gap-2"><div class="min-w-0 flex-1"><Toggle label={toggle.label} checked={draft.dither[toggle.key]} disabled={busy} onChange={(checked) => { draft.dither[toggle.key] = checked; }} /></div><ResetSetting label={t("Ripristina {0}", $language, [toggle.label])} disabled={busy || draft.dither[toggle.key] === defaultDither()[toggle.key]} onClick={() => { draft.dither[toggle.key] = defaultDither()[toggle.key]; }} /></div>
        {/each}
      </fieldset>
    {/if}
  </SettingsGroup>

  <SettingsGroup title={t("Pannelli e dialoghi", $language)} icon="landscape" collapsible>
    <div class="grid gap-3 sm:grid-cols-2">
      {#each surfaceEffects as effect (effect.key)}
        <AppearanceSlider id={"theme-" + effect.key} label={effect.label} value={draft[effect.key]} min={effect.min} max={effect.max} unit={effect.unit} resetValue={defaultAppearance()[effect.key]} disabled={busy} onChange={(value) => { draft[effect.key] = value; }} />
      {/each}
    </div>
  </SettingsGroup>

  {#if hyprland}
    <SettingsGroup title={t("Trasparenza della finestra", $language)} icon="image" collapsible>
      <div class="flex items-center gap-2"><div class="min-w-0 flex-1"><Toggle label={t("Trasparenza della finestra su Hyprland", $language)} checked={draft.transparent} disabled={busy} onChange={(checked) => { draft.transparent = checked; }} /></div><ResetSetting label={t("Ripristina trasparenza", $language)} disabled={busy || !draft.transparent} onClick={() => { draft.transparent = false; }} /></div>
    </SettingsGroup>
  {/if}

  <div class="flex flex-wrap items-center justify-between gap-3 pt-4">
    <Button variant="secondary" label={t("Ripristina aspetto predefinito", $language)} disabled={busy} onClick={reset} />
  </div>
</section>

<style>
  .appearance-settings { min-width: 0; }
</style>
