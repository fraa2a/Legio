<script lang="ts">
  import { get } from "svelte/store";
  import { onDestroy, onMount } from "svelte";
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
  import Toggle from "../../components/ui/Toggle.svelte";
  import ResetSetting from "../../components/ui/ResetSetting.svelte";
  import Icon from "../../components/ui/Icon.svelte";

  const initial = get(settings).data;
  let theme: Theme = $state(initial.theme);
  let draft: Appearance = $state(structuredClone(initial.appearance));
  let baseline = $state(JSON.stringify({ theme: initial.theme, appearance: initial.appearance }));
  let failedSnapshot: string | null = null;
  let savingTask: Promise<void> | null = null;
  let busy = $state(false);
  let error = $state<string | null>(null);
  let notice = $state<string | null>(null);
  let disposed = false;
  let prefersDark = $state(window.matchMedia("(prefers-color-scheme: dark)").matches);
  let expandedCustomId = $state<string | null>(null);

  const serialized = $derived(JSON.stringify({ theme, appearance: draft }));
  const dirty = $derived(serialized !== baseline);
  const hyprland = $derived($appInfo.data.platform === "linux" && $appInfo.data.desktopEnvironment === "hyprland");
  const palette = $derived(activePalette(theme, draft, prefersDark));
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

  $effect(() => { if (dirty) notice = null; });

  $effect(() => {
    if (!dirty || busy || serialized === failedSnapshot) return;
    const timer = setTimeout(() => void save(), 450);
    return () => clearTimeout(timer);
  });

  onMount(() => {
    const media = window.matchMedia("(prefers-color-scheme: dark)");
    const changed = () => { prefersDark = media.matches; };
    media.addEventListener("change", changed);
    return () => media.removeEventListener("change", changed);
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

  function selectTheme(value: Theme, id: string | null = null): void {
    if (value !== "custom" || (id !== null && id !== expandedCustomId)) expandedCustomId = null;
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
    expandedCustomId = id;
  }

  function expandCustom(id: string): void {
    if (expandedCustomId === id) {
      expandedCustomId = null;
      return;
    }
    selectTheme("custom", id);
    expandedCustomId = id;
  }

  function updateCustom(changes: Partial<{ name: string; scheme: "dark" | "light"; palette: Palette }>): void {
    draft.customThemes = draft.customThemes.map((custom) =>
      custom.id === draft.customThemeId ? { ...custom, ...changes } : custom);
  }

  function removeCustom(): void {
    expandedCustomId = null;
    draft.customThemes = draft.customThemes.filter((custom) => custom.id !== draft.customThemeId);
    draft.customThemeId = draft.customThemes[0]?.id ?? null;
    theme = draft.customThemeId === null ? "dark" : "custom";
  }

  function save(): Promise<void> {
    if (savingTask !== null) return savingTask;
    const snapshot = serialized;
    const selectedTheme = theme;
    const selectedAppearance = structuredClone($state.snapshot(draft));
    savingTask = (async () => {
      busy = true;
      error = null;
      try {
        const saved = await saveSettings({ ...get(settings).data, theme: selectedTheme, appearance: selectedAppearance });
        settings.set(saved);
        theme = saved.theme;
        draft = structuredClone(saved.appearance);
        baseline = JSON.stringify({ theme: saved.theme, appearance: saved.appearance });
        failedSnapshot = null;
        notice = t("Aspetto salvato.", get(language));
        await cleanupBackgrounds();
      } catch (reason) {
        failedSnapshot = snapshot;
        error = toMessage(reason);
      } finally {
        busy = false;
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
    expandedCustomId = null;
    draft = { ...defaultAppearance(), customThemes: draft.customThemes };
    notice = null;
  }
</script>

<section class="appearance-settings flex flex-col gap-5">
  {#if error}<ErrorBanner message={error} />{/if}
  {#if notice}<p class="text-sm text-emerald-400 light:text-emerald-700" role="status">{notice}</p>{/if}

  <SettingsGroup title={t("Tema", $language)} description={t("Scegli una palette. Le modifiche vengono salvate automaticamente.", $language)}>
    <div class="flex items-center justify-between gap-3">
      <span class="text-xs text-zinc-500 light:text-zinc-600">{t("Palette disponibili", $language)}</span>
      <button type="button" class="grid size-8 place-items-center rounded-lg border border-white/10 text-zinc-300 transition-colors hover:border-legio-accent hover:text-legio-accent disabled:opacity-40 light:border-zinc-900/15 light:text-zinc-700" aria-label={t("Crea tema dalla palette attuale", $language)} title={t("Crea tema dalla palette attuale", $language)} disabled={busy || draft.customThemes.length >= 20} onclick={createCustom}><Icon name="plus" size="size-4" /></button>
    </div>
    <fieldset disabled={busy} class="grid grid-cols-2 gap-2 lg:grid-cols-3">
      <legend class="sr-only">{t("Palette predefinite", $language)}</legend>
      <label class="flex min-h-16 cursor-pointer items-center gap-2 rounded-lg border px-3 py-2 transition-colors focus-within:outline-2 focus-within:outline-legio-accent {theme === 'system' ? 'border-legio-accent bg-legio-accent/10' : 'border-white/10 hover:bg-white/5 light:border-zinc-900/15 light:hover:bg-zinc-900/5'}">
        <input class="sr-only" type="radio" name="theme" checked={theme === "system"} onchange={() => selectTheme("system")} />
        <span class="size-3 shrink-0 rounded-full border border-zinc-400 bg-gradient-to-br from-zinc-800 to-zinc-100" aria-hidden="true"></span>
        <span class="truncate text-sm text-zinc-100 light:text-zinc-900">{t("Sistema", $language)}</span>
      </label>
      {#each themePresets as preset (preset.id)}
        <label class="flex min-h-16 cursor-pointer items-center gap-2 rounded-lg border px-3 py-2 transition-colors focus-within:outline-2 focus-within:outline-legio-accent {theme === preset.id ? 'border-legio-accent bg-legio-accent/10' : 'border-white/10 hover:bg-white/5 light:border-zinc-900/15 light:hover:bg-zinc-900/5'}">
          <input class="sr-only" type="radio" name="theme" checked={theme === preset.id} onchange={() => selectTheme(preset.id as Theme)} />
          <span class="flex shrink-0 -space-x-1" aria-hidden="true"><span class="size-4 rounded-full border border-white/20" style:background-color={preset.palette.background}></span><span class="size-4 rounded-full border border-white/20" style:background-color={preset.palette.accent}></span></span>
          <span class="truncate text-sm text-zinc-100 light:text-zinc-900">{preset.id === "eggplant" ? preset.name : t(preset.name, $language)}</span>
        </label>
      {/each}
      {#each draft.customThemes as custom (custom.id)}
        <div class="flex min-h-16 items-center gap-1 rounded-lg border px-2 transition-colors {theme === 'custom' && draft.customThemeId === custom.id ? 'border-legio-accent bg-legio-accent/10' : 'border-white/10 light:border-zinc-900/15'}">
          <label class="flex min-w-0 flex-1 cursor-pointer items-center gap-2 py-2 focus-within:outline-2 focus-within:outline-legio-accent">
            <input class="sr-only" type="radio" name="theme" checked={theme === "custom" && draft.customThemeId === custom.id} disabled={busy} onchange={() => selectTheme("custom", custom.id)} />
            <span class="size-4 shrink-0 rounded-full border border-white/20" style:background-color={custom.palette.accent} aria-hidden="true"></span>
            <span class="truncate text-sm text-zinc-100 light:text-zinc-900">{custom.name}</span>
          </label>
          <button type="button" class="grid size-8 shrink-0 place-items-center rounded-md text-zinc-400 transition-colors hover:bg-white/10 hover:text-zinc-100 light:hover:bg-zinc-900/10 light:hover:text-zinc-900" aria-label={t("Modifica la palette {0}", $language, [custom.name])} aria-expanded={expandedCustomId === custom.id} disabled={busy} onclick={() => expandCustom(custom.id)}><span class="transition-transform {expandedCustomId === custom.id ? 'rotate-180' : ''}"><Icon name="chevron-down" size="size-4" /></span></button>
        </div>
      {/each}
    </fieldset>
    {#if editable && expandedCustomId === draft.customThemeId}
      <div class="mt-2 flex flex-col gap-4 border-t border-white/10 pt-4 light:border-zinc-900/10">
        <div class="grid gap-3 sm:grid-cols-2">
          <div class="flex items-end gap-2"><div class="min-w-0 flex-1"><TextField id="theme-name" label={t("Nome del tema", $language)} value={palette.name} disabled={busy} oninput={(name) => updateCustom({ name })} /></div><ResetSetting label={t("Ripristina nome del tema", $language)} disabled={busy || palette.name === t("Il mio tema", $language)} onClick={() => updateCustom({ name: t("Il mio tema", $language) })} /></div>
          <div class="flex items-end gap-2"><div class="min-w-0 flex-1"><SelectField id="theme-scheme" label={t("Schema dei controlli", $language)} value={palette.scheme} options={[{ value: "dark", label: t("Scuro", $language) }, { value: "light", label: t("Chiaro", $language) }]} disabled={busy} onChange={(scheme) => { if (scheme === "dark" || scheme === "light") updateCustom({ scheme }); }} /></div><ResetSetting label={t("Ripristina schema del tema", $language)} disabled={busy || palette.scheme === "dark"} onClick={() => updateCustom({ scheme: "dark" })} /></div>
        </div>
        <fieldset disabled={busy} class="grid gap-3 sm:grid-cols-2">
          <legend class="mb-2 text-xs font-medium text-zinc-400 light:text-zinc-600">{t("Colori della palette", $language)}</legend>
          {#each colorFields as field (field.key)}
            <div class="flex items-center gap-1">
              <label for={"theme-color-" + field.key} class="flex min-w-0 flex-1 items-center justify-between gap-2 text-sm text-zinc-300 light:text-zinc-700"><span>{field.label}</span><input id={"theme-color-" + field.key} type="color" value={palette.palette[field.key]} class="h-9 w-12 cursor-pointer rounded border border-white/15 bg-transparent light:border-zinc-900/15" oninput={(event) => updateCustom({ palette: { ...palette.palette, [field.key]: event.currentTarget.value } })} /></label>
              <ResetSetting label={t("Ripristina {0}", $language, [field.label])} disabled={busy} onClick={() => updateCustom({ palette: { ...palette.palette, [field.key]: themePresets.find((preset) => preset.id === palette.scheme)?.palette[field.key] ?? palette.palette[field.key] } })} />
            </div>
          {/each}
        </fieldset>
        {#if !readablePalette(palette)}<p class="text-sm text-amber-400 light:text-amber-700" role="status">{t("Contrasto insufficiente o schema incoerente: il launcher usa una palette leggibile.", $language)}</p>{/if}
        <p class="text-xs text-zinc-500">{t("Il testo deve rimanere leggibile su sfondo e pannelli. I temi con contrasto insufficiente non vengono salvati.", $language)}</p>
        <Button variant="danger" label={t("Elimina questo tema", $language)} disabled={busy} onClick={removeCustom} />
      </div>
    {/if}
  </SettingsGroup>

  <SettingsGroup title={t("Sfondo ed effetti", $language)} description={t("Lo sfondo viene copiato in Legio e incluso nei temi esportati.", $language)}>
    <div class="flex flex-wrap gap-2">
      <Button variant="secondary" label={t("Scegli uno sfondo", $language)} disabled={busy} onClick={() => void selectBackground()} />
      <Button variant="secondary" label={t("Rimuovi sfondo", $language)} disabled={busy || draft.background === null} onClick={() => { draft.background = null; }} />
    </div>
    <p class="text-xs text-zinc-500">{t("PNG, JPEG o WebP, fino a 16 MiB e 16 megapixel.", $language)}</p>
    <fieldset disabled={busy} class="grid gap-2 sm:grid-cols-3">
      <legend class="mb-2 text-sm font-medium text-zinc-100 light:text-zinc-900">{t("Sfondi animati", $language)}</legend>
      {#each [{ id: "none", label: t("Nessuno", $language) }, { id: "particles", label: t("Punti connessi", $language) }, { id: "aurora", label: t("Aurora", $language) }] as option (option.id)}
        <label class="flex cursor-pointer items-center gap-2 rounded-xl border p-3 text-sm transition-colors focus-within:outline-2 focus-within:outline-offset-2 focus-within:outline-legio-accent {draft.animatedBackground === option.id ? 'border-legio-accent bg-legio-accent/10 text-zinc-100 light:text-zinc-900' : 'border-white/10 text-zinc-400 hover:bg-white/5 light:border-zinc-900/15 light:text-zinc-600'}">
          <input class="sr-only" type="radio" name="animated-background" checked={draft.animatedBackground === option.id} onchange={() => { draft.animatedBackground = option.id as Appearance["animatedBackground"]; }} />
          <span class="grid size-4 shrink-0 place-items-center rounded-full border-2 {draft.animatedBackground === option.id ? 'border-legio-accent' : 'border-zinc-500'}" aria-hidden="true">{#if draft.animatedBackground === option.id}<span class="size-2 rounded-full bg-legio-accent"></span>{/if}</span>
          {option.label}
        </label>
      {/each}
    </fieldset>
    <ResetSetting label={t("Ripristina sfondo animato", $language)} disabled={busy || draft.animatedBackground === "none"} onClick={() => { draft.animatedBackground = "none"; }} />
    <div class="flex flex-col gap-2 text-sm text-zinc-300 light:text-zinc-700">
      <div class="flex items-center justify-between gap-3"><label for="theme-animated-opacity">{t("Opacità dell'animazione", $language)}</label><span class="flex items-center"><output for="theme-animated-opacity">{draft.animatedOpacity} %</output><ResetSetting label={t("Ripristina opacità dell'animazione", $language)} disabled={busy || draft.animatedOpacity === 65} onClick={() => { draft.animatedOpacity = 65; }} /></span></div>
      <input id="theme-animated-opacity" type="range" min="0" max="100" value={draft.animatedOpacity} disabled={busy || draft.animatedBackground === "none"}
        oninput={(event) => { draft.animatedOpacity = Number(event.currentTarget.value); }} />
    </div>
    {#each effects as effect (effect.key)}
      <div class="flex flex-col gap-2 text-sm text-zinc-300 light:text-zinc-700">
        <div class="flex items-center justify-between gap-3"><label for={"theme-" + effect.key}>{effect.label}</label><span class="flex items-center"><output for={"theme-" + effect.key}>{draft[effect.key]} {effect.unit}</output><ResetSetting label={t("Ripristina {0}", $language, [effect.label])} disabled={busy || draft[effect.key] === defaultAppearance()[effect.key]} onClick={() => { draft[effect.key] = defaultAppearance()[effect.key]; }} /></span></div>
        <input id={"theme-" + effect.key} type="range" min={effect.min} max={effect.max} step="1" value={draft[effect.key]}
          disabled={busy || (effect.key !== "surfaceOpacity" && draft.background === null)}
          oninput={(event) => { draft[effect.key] = Number(event.currentTarget.value); }} />
      </div>
    {/each}
    {#if hyprland}
      <div class="flex items-center gap-2"><div class="min-w-0 flex-1"><Toggle label={t("Trasparenza della finestra su Hyprland", $language)} checked={draft.transparent} disabled={busy} onChange={(checked) => { draft.transparent = checked; }} /></div><ResetSetting label={t("Ripristina trasparenza", $language)} disabled={busy || !draft.transparent} onClick={() => { draft.transparent = false; }} /></div>
      <p class="text-xs text-zinc-500">{t("Il blur qui modifica l'immagine. Il blur del desktop dietro la finestra dipende dalle regole di Hyprland.", $language)}</p>
    {/if}
  </SettingsGroup>

  <SettingsGroup title={t("Importa ed esporta", $language)}>
    <p class="text-xs text-zinc-500">{t("I file JSON includono palette, sfondo ed effetti. Attendi il salvataggio automatico, poi scegli un nuovo file per esportare.", $language)}</p>
    <div class="flex flex-wrap gap-2">
      <Button variant="secondary" label={t("Importa tema JSON", $language)} disabled={busy || dirty || draft.customThemes.length >= 20} onClick={() => void importFile()} />
      <Button variant="secondary" label={t("Esporta tema JSON", $language)} disabled={busy || dirty} onClick={() => void exportFile()} />
    </div>
  </SettingsGroup>

  <div class="flex flex-wrap items-center justify-between gap-3 border-t border-white/10 pt-4 light:border-zinc-900/10">
    <Button variant="secondary" label={t("Ripristina aspetto predefinito", $language)} disabled={busy} onClick={reset} />
    {#if busy}<span class="text-xs text-zinc-400 light:text-zinc-600">{t("Salvataggio...", $language)}</span>{/if}
  </div>
</section>
