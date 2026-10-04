<script lang="ts">
  import { get } from "svelte/store";
  import { onDestroy } from "svelte";
  import { t, language } from "../../i18n";
  import type { Theme } from "../../services/local-state";
  import { saveSettings } from "../../services/local-state";
  import {
    activePalette, chooseBackground, cleanupBackgrounds, defaultAppearance, exportTheme, importTheme,
    readablePalette, themePresets, type Appearance, type Palette,
  } from "../../services/appearance";
  import { appearancePreview } from "../../stores/appearance";
  import { appInfo } from "../../stores/app-info";
  import { settings } from "../../stores/settings";
  import { toMessage } from "../../utils/errors";
  import Button from "../../components/ui/Button.svelte";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import SelectField from "../../components/ui/SelectField.svelte";
  import SettingsGroup from "../../components/ui/SettingsGroup.svelte";
  import TextField from "../../components/ui/TextField.svelte";

  const initial = get(settings).data;
  let theme: Theme = $state(initial.theme);
  let draft: Appearance = $state(structuredClone(initial.appearance));
  let baseline = $state(JSON.stringify({ theme: initial.theme, appearance: initial.appearance }));
  let busy = $state(false);
  let error = $state<string | null>(null);
  let notice = $state<string | null>(null);
  let disposed = false;

  const dirty = $derived(JSON.stringify({ theme, appearance: draft }) !== baseline);
  const hyprland = $derived($appInfo.data.platform === "linux" && $appInfo.data.desktopEnvironment === "hyprland");
  const palette = $derived(activePalette(theme, draft, window.matchMedia("(prefers-color-scheme: dark)").matches));
  const editable = $derived(theme === "custom" && draft.customThemeId !== null);
  const colorFields: { key: keyof Palette; label: string }[] = $derived([
    { key: "background", label: t("Sfondo", $language) },
    { key: "surface", label: t("Pannelli", $language) },
    { key: "raised", label: t("Superfici in rilievo", $language) },
    { key: "text", label: t("Testo", $language) },
    { key: "muted", label: t("Testo secondario", $language) },
    { key: "accent", label: t("Accento", $language) },
  ]);
  const effects: { key: "backgroundBlur" | "backgroundOpacity" | "surfaceOpacity"; label: string; min: number; max: number; unit: string }[] = $derived([
    { key: "backgroundBlur", label: t("Blur dello sfondo", $language), min: 0, max: 40, unit: "px" },
    { key: "backgroundOpacity", label: t("Opacità dell'immagine", $language), min: 0, max: 100, unit: "%" },
    { key: "surfaceOpacity", label: t("Opacità dei pannelli", $language), min: 0, max: 100, unit: "%" },
  ]);

  $effect(() => {
    appearancePreview.set({ theme, appearance: structuredClone($state.snapshot(draft)) });
  });

  onDestroy(() => {
    disposed = true;
    appearancePreview.set(null);
    // Keep the saved asset; discard abandoned image previews.
    void cleanupBackgrounds().catch((reason) => console.error("Could not clean up theme previews", reason));
  });

  function selectTheme(value: Theme, id: string | null = null): void {
    theme = value;
    if (id !== null) draft.customThemeId = id;
    notice = null;
  }

  function createCustom(): void {
    const id = crypto.randomUUID();
    draft.customThemes = [...draft.customThemes, {
      ...structuredClone($state.snapshot(palette)), id, name: t("Il mio tema", $language),
    }];
    draft.customThemeId = id;
    theme = "custom";
  }

  function updateCustom(changes: Partial<{ name: string; scheme: "dark" | "light"; palette: Palette }>): void {
    draft.customThemes = draft.customThemes.map((custom) =>
      custom.id === draft.customThemeId ? { ...custom, ...changes } : custom);
  }

  function removeCustom(): void {
    draft.customThemes = draft.customThemes.filter((custom) => custom.id !== draft.customThemeId);
    draft.customThemeId = draft.customThemes[0]?.id ?? null;
    theme = draft.customThemeId === null ? "dark" : "custom";
  }

  async function save(): Promise<void> {
    busy = true;
    error = null;
    notice = null;
    try {
      const saved = await saveSettings({
        ...get(settings).data, theme, appearance: structuredClone($state.snapshot(draft)),
      });
      settings.set(saved);
      baseline = JSON.stringify({ theme: saved.theme, appearance: saved.appearance });
      notice = t("Aspetto salvato.", get(language));
      await cleanupBackgrounds();
    } catch (reason) {
      error = toMessage(reason);
    } finally {
      busy = false;
    }
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

  async function importFile(): Promise<void> {
    busy = true;
    error = null;
    notice = null;
    try {
      const imported = await importTheme();
      if (imported !== null) {
        settings.set(imported);
        if (!disposed) {
          theme = imported.theme;
          draft = structuredClone(imported.appearance);
          baseline = JSON.stringify({ theme, appearance: imported.appearance });
          notice = t("Tema importato e applicato.", get(language));
        }
        await cleanupBackgrounds();
      }
    } catch (reason) {
      error = toMessage(reason);
    } finally {
      busy = false;
    }
  }

  async function exportFile(): Promise<void> {
    busy = true;
    error = null;
    notice = null;
    try {
      if (await exportTheme()) notice = t("Tema esportato.", get(language));
    } catch (reason) {
      error = toMessage(reason);
    } finally {
      busy = false;
    }
  }

  function reset(): void {
    theme = "system";
    draft = { ...defaultAppearance(), customThemes: draft.customThemes };
    notice = null;
  }
</script>

<section class="flex flex-col gap-5">
  {#if error}<ErrorBanner message={error} />{/if}
  {#if notice}<p class="text-sm text-emerald-400 light:text-emerald-700" role="status">{notice}</p>{/if}

  <div class="theme-preview" role="img" aria-label={t("Anteprima della palette e degli effetti", $language)}>
    <div class="theme-preview-wallpaper"></div>
    <div class="relative grid h-full grid-cols-[5rem_1fr] gap-2 p-3">
      <div class="theme-preview-panel flex flex-col gap-2 rounded-lg p-3">
        <span class="text-xs font-bold" style:color="var(--legio-text)">LEGIO</span>
        <span class="h-2 rounded" style:background="var(--legio-accent)"></span>
        <span class="h-2 rounded" style:background="var(--legio-muted)"></span>
        <span class="h-2 rounded" style:background="var(--legio-muted)"></span>
      </div>
      <div class="theme-preview-panel flex flex-col justify-center gap-2 rounded-lg p-3">
        <span class="text-sm font-medium" style:color="var(--legio-text)">{t("Libreria", $language)}</span>
        <span class="text-xs" style:color="var(--legio-muted)">{t("Anteprima", $language)}</span>
        <span class="w-fit rounded-md px-3 py-1 text-xs" style:background="var(--legio-accent)" style:color="var(--legio-accent-text)">{t("Gioca", $language)}</span>
      </div>
    </div>
  </div>

  <SettingsGroup title={t("Tema", $language)} description={t("Le modifiche sono mostrate in anteprima. Salva per mantenerle al prossimo avvio.", $language)}>
    <fieldset disabled={busy} class="grid grid-cols-2 gap-2">
      <legend class="sr-only">{t("Palette predefinite", $language)}</legend>
      <label class="flex cursor-pointer items-center gap-3 rounded-xl border border-white/10 p-3 light:border-zinc-900/15">
        <input type="radio" name="theme" checked={theme === "system"} onchange={() => selectTheme("system")} />
        <span class="text-sm text-zinc-100 light:text-zinc-900">{t("Sistema", $language)}</span>
      </label>
      {#each themePresets as preset (preset.id)}
        <label class="flex cursor-pointer items-center gap-3 rounded-xl border border-white/10 p-3 light:border-zinc-900/15" style:border-color={theme === preset.id ? preset.palette.accent : undefined}>
          <input type="radio" name="theme" checked={theme === preset.id} onchange={() => selectTheme(preset.id as Theme)} />
          <span class="flex min-w-0 flex-1 flex-col gap-2">
            <span class="text-sm text-zinc-100 light:text-zinc-900">{preset.id === "eggplant" ? preset.name : t(preset.name, $language)}</span>
            <span class="flex gap-1" aria-hidden="true">
              {#each [preset.palette.background, preset.palette.surface, preset.palette.text, preset.palette.accent] as color, index (index)}
                <span class="size-3.5 rounded-full border border-white/20" style:background-color={color}></span>
              {/each}
            </span>
          </span>
        </label>
      {/each}
      {#each draft.customThemes as custom (custom.id)}
        <label class="flex cursor-pointer items-center gap-3 rounded-xl border border-white/10 p-3 light:border-zinc-900/15">
          <input type="radio" name="theme" checked={theme === "custom" && draft.customThemeId === custom.id} onchange={() => selectTheme("custom", custom.id)} />
          <span class="truncate text-sm text-zinc-100 light:text-zinc-900">{custom.name}</span>
          <span class="ml-auto size-3.5 shrink-0 rounded-full" style:background-color={custom.palette.accent} aria-hidden="true"></span>
        </label>
      {/each}
    </fieldset>
    <Button variant="secondary" label={t("Crea tema dalla palette attuale", $language)} disabled={busy || draft.customThemes.length >= 20} onClick={createCustom} />
  </SettingsGroup>

  {#if editable}
    <SettingsGroup title={t("Palette personalizzata", $language)}>
      <TextField id="theme-name" label={t("Nome del tema", $language)} value={palette.name} disabled={busy} oninput={(name) => updateCustom({ name })} />
      <SelectField id="theme-scheme" label={t("Schema dei controlli", $language)} value={palette.scheme}
        options={[{ value: "dark", label: t("Scuro", $language) }, { value: "light", label: t("Chiaro", $language) }]}
        disabled={busy} onChange={(scheme) => { if (scheme === "dark" || scheme === "light") updateCustom({ scheme }); }} />
      <fieldset disabled={busy} class="grid grid-cols-2 gap-3">
        <legend class="sr-only">{t("Colori della palette", $language)}</legend>
        {#each colorFields as field (field.key)}
          <label class="flex items-center justify-between gap-2 text-sm text-zinc-300 light:text-zinc-700">
            <span>{field.label}</span>
            <input type="color" value={palette.palette[field.key]} class="h-9 w-12 cursor-pointer rounded border border-white/15 bg-transparent light:border-zinc-900/15"
              oninput={(event) => updateCustom({ palette: { ...palette.palette, [field.key]: event.currentTarget.value } })} />
          </label>
        {/each}
      </fieldset>
      {#if !readablePalette(palette)}<p class="text-sm text-amber-400 light:text-amber-700" role="status">{t("Contrasto insufficiente o schema incoerente: l'anteprima usa una palette leggibile.", $language)}</p>{/if}
      <p class="text-xs text-zinc-500">{t("Il testo deve rimanere leggibile su sfondo e pannelli. I temi con contrasto insufficiente non vengono salvati.", $language)}</p>
      <Button variant="danger" label={t("Elimina questo tema", $language)} disabled={busy} onClick={removeCustom} />
    </SettingsGroup>
  {/if}

  <SettingsGroup title={t("Sfondo ed effetti", $language)} description={t("Lo sfondo viene copiato in Legio e incluso nei temi esportati.", $language)}>
    <div class="flex flex-wrap gap-2">
      <Button variant="secondary" label={t("Scegli uno sfondo", $language)} disabled={busy} onClick={() => void selectBackground()} />
      <Button variant="secondary" label={t("Rimuovi sfondo", $language)} disabled={busy || draft.background === null} onClick={() => { draft.background = null; }} />
    </div>
    <p class="text-xs text-zinc-500">{t("PNG, JPEG o WebP, fino a 16 MiB e 16 megapixel.", $language)}</p>
    {#each effects as effect (effect.key)}
      <label class="flex flex-col gap-2 text-sm text-zinc-300 light:text-zinc-700">
        <span class="flex justify-between gap-3"><span>{effect.label}</span><output>{draft[effect.key]} {effect.unit}</output></span>
        <input type="range" min={effect.min} max={effect.max} step="1" value={draft[effect.key]}
          disabled={busy || (effect.key !== "surfaceOpacity" && draft.background === null)}
          oninput={(event) => { draft[effect.key] = Number(event.currentTarget.value); }} />
      </label>
    {/each}
    {#if hyprland}
      <label class="flex items-center gap-3 text-sm text-zinc-100 light:text-zinc-900">
        <input type="checkbox" checked={draft.transparent} disabled={busy} onchange={(event) => { draft.transparent = event.currentTarget.checked; }} />
        <span>{t("Trasparenza della finestra su Hyprland", $language)}</span>
      </label>
      <p class="text-xs text-zinc-500">{t("Il blur qui modifica l'immagine. Il blur del desktop dietro la finestra dipende dalle regole di Hyprland.", $language)}</p>
    {/if}
  </SettingsGroup>

  <SettingsGroup title={t("Importa ed esporta", $language)}>
    <p class="text-xs text-zinc-500">{t("I file JSON includono palette, sfondo ed effetti. Per esportare, salva prima le modifiche e scegli un nuovo file.", $language)}</p>
    <div class="flex flex-wrap gap-2">
      <Button variant="secondary" label={t("Importa tema JSON", $language)} disabled={busy || dirty || draft.customThemes.length >= 20} onClick={() => void importFile()} />
      <Button variant="secondary" label={t("Esporta tema JSON", $language)} disabled={busy || dirty} onClick={() => void exportFile()} />
    </div>
  </SettingsGroup>

  <div class="sticky bottom-0 flex flex-wrap items-center gap-3 rounded-xl bg-zinc-900 p-3 light:bg-zinc-50">
    <Button label={busy ? t("Salvataggio...", $language) : t("Salva aspetto", $language)} disabled={busy || !dirty} onClick={() => void save()} />
    <Button variant="secondary" label={t("Ripristina aspetto predefinito", $language)} disabled={busy} onClick={reset} />
    {#if dirty}<span class="text-xs text-zinc-400 light:text-zinc-600">{t("Modifiche non salvate", $language)}</span>{/if}
  </div>
</section>


<style>
  .theme-preview {
    position: relative; height: 9rem; overflow: hidden; border-radius: 1rem;
    background-color: var(--legio-background);
    border: 1px solid color-mix(in srgb, var(--legio-text) 15%, transparent);
  }
  .theme-preview-wallpaper {
    position: absolute; inset: 0; background-image: var(--legio-wallpaper-image, none);
    background-size: cover; background-position: center; transform: scale(1.1);
    filter: blur(var(--legio-image-blur)); opacity: var(--legio-image-opacity);
  }
  .theme-preview-panel {
    background-color: color-mix(in srgb, var(--legio-surface) calc(var(--legio-surface-opacity) * 100%), transparent);
  }
</style>
