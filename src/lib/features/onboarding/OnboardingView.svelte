<script lang="ts">
  import { onMount, tick } from "svelte";
  import { t, language } from "../../i18n";
  import { themePresets } from "../../services/appearance";
  import Button from "../../components/ui/Button.svelte";
  import Toggle from "../../components/ui/Toggle.svelte";
  import SelectField from "../../components/ui/SelectField.svelte";
  import TextField from "../../components/ui/TextField.svelte";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import { compatibilityRunnerLabel } from "../../services/game-settings";
  import { onboarding, startOnboarding, changeOnboarding, changeCompatibility, chooseOnboardingDirectory, completeOnboarding, endOnboardingPreview } from "../../stores/onboarding";
  import { appInfo } from "../../stores/app-info";
  let step = $state(0);
  const linux = $derived($appInfo.data.platform === "linux");
  const steps = $derived(["Benvenuto in Legio", "Il tuo stile", "I tuoi giochi", "Come vuoi usare Legio", ...(linux ? ["Compatibilità Linux"] : []), "Tutto pronto"]);
  onMount(() => { void startOnboarding(); return endOnboardingPreview; });
  async function moveStep(delta: number) { step += delta; await tick(); document.getElementById("onboarding-title")?.focus(); }
</script>

<main class="legio-content min-h-0 flex-1 overflow-auto rounded-xl bg-black p-6 light:bg-zinc-50" aria-labelledby="onboarding-title">
  <section class="mx-auto flex max-w-2xl flex-col gap-6 rounded-2xl legio-glass p-6">
    <header>
      <p class="text-sm text-zinc-400 light:text-zinc-600">{t("Configurazione iniziale", $language)} · {step + 1} / {steps.length}</p>
      <h1 tabindex="-1" id="onboarding-title" class="mt-2 text-3xl font-semibold text-zinc-50 light:text-zinc-900">{t(steps[step], $language)}</h1>
      <p class="mt-2 text-sm text-zinc-400 light:text-zinc-600">{t("Potrai cambiare tutte queste scelte nelle impostazioni.", $language)}</p>
    </header>
    {#if $onboarding.error}<ErrorBanner message={$onboarding.error} />{/if}
    {#if $onboarding.draft}
      {#if step === 0}
        <p class="text-sm text-zinc-300 light:text-zinc-700">{t("Configuriamo insieme le impostazioni principali del launcher.", $language)}</p>
        <SelectField id="onboarding-language" label={t("Lingua dell'interfaccia", $language)} value={$onboarding.draft.language} disabled={$onboarding.saving}
          options={[{ value: "system", label: t("Sistema", $language) }, { value: "it", label: "Italiano" }, { value: "en", label: "English" }]}
          onChange={(value) => { if (value === "system" || value === "it" || value === "en") changeOnboarding({ language: value }); }} />
      {:else if step === 1}
        <SelectField id="onboarding-theme" label={t("Tema", $language)} value={$onboarding.draft.theme} disabled={$onboarding.saving}
          options={[{ value: "system", label: t("Sistema", $language) }, ...themePresets.map((preset) => ({ value: preset.id, label: t(preset.name, $language) }))]}
          onChange={(value) => { if (value === "system" || value === "dark" || value === "light") changeOnboarding({ theme: value }); }} />
        <p class="text-sm text-zinc-400 light:text-zinc-600">{t("Sfondi, animazioni e trasparenza sono disponibili nelle impostazioni Aspetto.", $language)}</p>
      {:else if step === 2}
        <p class="text-sm text-zinc-300 light:text-zinc-700">{t("Scegli dove salvare download e installazioni. Steam verrà rilevato automaticamente.", $language)}</p>
        <p class="break-all text-sm text-zinc-400 light:text-zinc-600">{$onboarding.draft.downloadPath ?? t("Cartella predefinita di Legio", $language)}</p>
        <div class="flex flex-wrap gap-2">
          <Button label={t("Imposta cartella...", $language)} variant="secondary" onClick={() => void chooseOnboardingDirectory()} />
          <Button label={t("Usa cartella predefinita", $language)} variant="secondary" onClick={() => changeOnboarding({ downloadPath: null })} />
        </div>
        <p class="text-sm text-zinc-400 light:text-zinc-600">{t("La verifica aiuta a rilevare archivi corrotti o alterati. Ti consigliamo di lasciarla attiva.", $language)}</p>
      {:else if step === 3}
        {#if $appInfo.data.trayAvailable}
        <Toggle label={t("Riduci nell'area di notifica alla chiusura", $language)} checked={$onboarding.draft.closeToTray} onChange={(checked) => changeOnboarding({ closeToTray: checked })} />
        <Toggle label={t("Nascondi Legio nell'area di notifica quando avvii un gioco", $language)} checked={$onboarding.draft.hideOnGameStart} onChange={(checked) => changeOnboarding({ hideOnGameStart: checked })} />
        {/if}
        <Toggle label={t("Avvia Legio all'accesso al sistema", $language)} checked={$onboarding.draft.launchOnSystemStart} onChange={(checked) => changeOnboarding({ launchOnSystemStart: checked, ...(!checked ? { launchMinimized: false } : {}) })} />
        <Toggle label={t("Apri Legio sulla Libreria", $language)} checked={$onboarding.draft.launchInLibrary} onChange={(checked) => changeOnboarding({ launchInLibrary: checked })} />
        <Toggle label={t("Notifica di sistema al termine del download", $language)} checked={$onboarding.draft.downloadNotifications} onChange={(checked) => changeOnboarding({ downloadNotifications: checked })} />
      {:else if linux && step === 4}
        <p class="text-sm text-zinc-300 light:text-zinc-700">{t("Per i giochi Windows scegli Proton o Wine. Puoi mantenere la selezione automatica e configurare ogni gioco in seguito.", $language)}</p>
        {#if $onboarding.loading}<p role="status">{t("Caricamento...", $language)}</p>{/if}
        {#if $onboarding.compatibility}
          <SelectField id="onboarding-runner" label={t("Runner", $language)} value={$onboarding.compatibility.runnerPath ?? ""} disabled={$onboarding.loading}
            options={[{ value: "", label: t("Scelta automatica", $language) }, ...($onboarding.compatibility.runnerPath && !$onboarding.runners.some((r) => r.path === $onboarding.compatibility?.runnerPath) ? [{ value: $onboarding.compatibility.runnerPath, label: $onboarding.compatibility.runnerPath }] : []), ...$onboarding.runners.map((runner) => ({ value: runner.path, label: compatibilityRunnerLabel(runner) }))]}
            onChange={(value) => changeCompatibility({ runnerPath: value || null })} />
          <TextField id="onboarding-prefix-root" label={t("Cartella predefinita dei prefix", $language)} value={$onboarding.compatibility.prefixRoot ?? ""} placeholder={t("Percorso opzionale", $language)} hint={t("Legio crea un prefix per gioco dentro questa cartella.", $language)} disabled={$onboarding.loading || $onboarding.saving} oninput={(value) => changeCompatibility({ prefixRoot: value || null })} />
          <div class="flex flex-wrap gap-2">
            <Button label={t("Scegli cartella prefix", $language)} variant="secondary" disabled={$onboarding.loading || $onboarding.saving} onClick={() => void chooseOnboardingDirectory(true)} />
            {#if $onboarding.compatibility.prefixRoot !== null}<Button label={t("Ripristina cartella prefix", $language)} variant="secondary" disabled={$onboarding.saving} onClick={() => changeCompatibility({ prefixRoot: null })} />{/if}
          </div>
          <Toggle label={t("Log di debug", $language)} checked={$onboarding.compatibility.debugLogging} onChange={(checked) => changeCompatibility({ debugLogging: checked })} />
        {/if}
        {#each $onboarding.diagnostics as diagnostic (diagnostic)}<p class="text-sm text-zinc-400 light:text-zinc-600">{diagnostic}</p>{/each}
        {#if !$onboarding.loading && $onboarding.runners.length === 0}<p class="text-sm text-zinc-400 light:text-zinc-600">{t("Nessun runner rilevato. Installa Proton tramite Steam o Wine per avviare giochi Windows.", $language)}</p>{/if}
      {:else}
        <p class="text-sm text-zinc-300 light:text-zinc-700">{t("Salva la configurazione e apri la tua libreria. Potrai aggiungere giochi manualmente o cercarli nello store.", $language)}</p>
        <dl class="grid grid-cols-2 gap-3 text-sm text-zinc-300 light:text-zinc-700">
          <dt>{t("Lingua", $language)}</dt><dd>{$onboarding.draft.language}</dd>
          <dt>{t("Tema", $language)}</dt><dd>{t(themePresets.find((preset) => preset.id === $onboarding.draft?.theme)?.name ?? $onboarding.draft.theme, $language)}</dd>
          <dt>{t("Cartella giochi", $language)}</dt><dd class="break-all">{$onboarding.draft.downloadPath ?? t("Cartella predefinita di Legio", $language)}</dd>
        </dl>
      {/if}
      <footer class="flex flex-wrap items-center justify-between gap-3">
        <Button label={t("Usa impostazioni attuali", $language)} variant="secondary" disabled={$onboarding.saving} onClick={() => void completeOnboarding(true)} />
        <Button label={t("Indietro", $language)} variant="secondary" disabled={step === 0 || $onboarding.saving} onClick={() => void moveStep(-1)} />
        {#if step === steps.length - 1}
          <Button label={$onboarding.saving ? t("Salvataggio...", $language) : t("Inizia a giocare", $language)} disabled={$onboarding.saving || $onboarding.loading} onClick={() => void completeOnboarding()} />
        {:else}
          <Button label={t("Continua", $language)} disabled={$onboarding.saving} onClick={() => void moveStep(1)} />
        {/if}
      </footer>
    {/if}
  </section>
</main>
