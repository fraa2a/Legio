<script lang="ts">
  import { t, language } from "../../i18n";
  import {
    browseGameDirectory,
    chooseCandidate,
    clearSelection,
    manualImport,
    pickExecutable,
    rescanCurrentDirectory,
    setScanGameName,
  } from "../../stores/manual-import";
  import Badge from "../../components/ui/Badge.svelte";
  import Button from "../../components/ui/Button.svelte";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import Spinner from "../../components/ui/Spinner.svelte";
  import TextField from "../../components/ui/TextField.svelte";
  import Callout from "../../components/ui/Callout.svelte";

  let {
    hint,
    startPath = null,
    nameLabel = t("Nome del gioco", $language),
    disabled = false,
  }: {
    hint: string;
    startPath?: string | null;
    nameLabel?: string;
    disabled?: boolean;
  } = $props();

  const group = $props.id();

  const pickedPath = $derived(
    $manualImport.selectedPath !== null &&
      !$manualImport.candidates.some((candidate) => candidate.path === $manualImport.selectedPath)
      ? $manualImport.selectedPath
      : null,
  );

  const signalLabels: Record<string, string> = $derived({
    game_name_match: t("nome del gioco", $language),
    game_root: t("cartella radice", $language),
  });

  function describeSignals(signals: string[]): string {
    return signals.map((signal) => signalLabels[signal] ?? signal).join(", ");
  }
</script>

<section class="flex flex-col gap-3">
  <p class="text-sm text-legio-fg-muted">{hint}</p>

  <TextField
    id="{group}-game-name"
    label={nameLabel}
    value={$manualImport.gameName}
    placeholder={t("Opzionale", $language)}
    {disabled}
    oninput={(value) => setScanGameName(value)}
  />

  <div class="flex flex-wrap gap-2">
    <Button
      label={t("Scegli cartella...", $language)}
      variant="secondary"
      {disabled}
      onClick={() => void browseGameDirectory(startPath)}
    />
    {#if $manualImport.directory !== null}
      <Button
        label={t("Riesegui scansione", $language)}
        variant="secondary"
        {disabled}
        onClick={() => void rescanCurrentDirectory()}
      />
    {/if}
    <Button
      label={t("Scegli eseguibile...", $language)}
      variant="secondary"
      {disabled}
      onClick={() => void pickExecutable(startPath)}
    />
  </div>

  {#if $manualImport.directory !== null}
    <p class="break-all text-xs text-legio-fg-subtle">{$manualImport.directory}</p>
  {/if}

  {#if pickedPath !== null}
    <div
      class="flex flex-wrap items-center justify-between gap-2 rounded-lg bg-sky-500/10 p-3 text-sm text-sky-200 light:text-sky-900"
    >
      <span class="min-w-0 break-all">{t("Eseguibile scelto: {0}", $language, [pickedPath])}</span>
      <Button
        label={t("Annulla scelta", $language)}
        variant="secondary"
        {disabled}
        onClick={clearSelection}
      />
    </div>
  {/if}

  {#if $manualImport.status === "loading"}
    <p class="flex items-center gap-3 text-sm text-legio-fg" role="status">
      <Spinner />{t("Scansione degli eseguibili in corso...", $language)}</p>
  {/if}

  {#if $manualImport.status === "error" && $manualImport.error !== null}
    <ErrorBanner message={$manualImport.error} onRetry={() => void rescanCurrentDirectory()} />
  {/if}

  {#if $manualImport.status === "empty"}
    <p class="text-sm text-legio-fg-muted">{t("Nessun eseguibile trovato in questa cartella.", $language)}</p>
  {/if}

  {#if $manualImport.candidates.length > 0}
    {#if $manualImport.suggestedPath === null}
      <Callout tone="warning" message={t("La scansione non ha un candidato chiaro: scegli l'eseguibile da usare.", $language)} />
    {/if}
    <fieldset class="flex flex-col gap-2" {disabled}>
      <legend class="sr-only">{t("Candidati trovati", $language)}</legend>
      {#each $manualImport.candidates as candidate (candidate.path)}
        <label
          class="flex cursor-pointer items-start gap-3 rounded-lg bg-legio-fill p-3 has-checked:bg-legio-fill-hover"
        >
          <input
            type="radio"
            name={group}
            class="mt-1"
            checked={$manualImport.selectedPath === candidate.path}
            onchange={() => chooseCandidate(candidate.path)}
          />
          <span class="min-w-0 flex-1">
            <span class="block break-all text-sm text-legio-fg">
              {candidate.path}
            </span>
            <span class="mt-1 flex flex-wrap items-center gap-2 text-xs text-legio-fg-subtle">
              <Badge tone="neutral" title={t("Punteggio {0}", $language, [candidate.score])} />
              {#if candidate.signals.length > 0}
                <span>{describeSignals(candidate.signals)}</span>
              {/if}
            </span>
          </span>
        </label>
      {/each}
    </fieldset>
  {/if}
</section>
