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
  import TextField from "../../components/ui/TextField.svelte";

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
  <p class="text-sm text-zinc-400 light:text-zinc-600">{hint}</p>

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
    <p class="break-all text-xs text-zinc-500">{$manualImport.directory}</p>
  {/if}

  {#if pickedPath !== null}
    <div
      class="flex flex-wrap items-center justify-between gap-2 rounded-lg bg-sky-500/10 p-3 text-sm text-sky-200 light:text-sky-900"
    >
      <span class="min-w-0 break-all">{t("Eseguibile scelto: ", $language)}{pickedPath}</span>
      <Button
        label={t("Annulla scelta", $language)}
        variant="secondary"
        {disabled}
        onClick={clearSelection}
      />
    </div>
  {/if}

  {#if $manualImport.status === "loading"}
    <p class="flex items-center gap-3 text-sm text-zinc-300 light:text-zinc-700" role="status">
      <span
        class="size-4 shrink-0 animate-spin rounded-full border-2 border-zinc-400 border-t-transparent"
        aria-hidden="true"
      ></span>{t("\n      Scansione degli eseguibili in corso...\n    ", $language)}</p>
  {/if}

  {#if $manualImport.status === "error" && $manualImport.error !== null}
    <ErrorBanner message={$manualImport.error} onRetry={() => void rescanCurrentDirectory()} />
  {/if}

  {#if $manualImport.status === "empty"}
    <p class="text-sm text-zinc-400 light:text-zinc-600">{t("\n      Nessun eseguibile trovato in questa cartella.\n    ", $language)}</p>
  {/if}

  {#if $manualImport.candidates.length > 0}
    {#if $manualImport.suggestedPath === null}
      <p class="text-sm text-amber-300 light:text-amber-800">{t("\n        La scansione non ha un candidato chiaro: scegli l'eseguibile da usare.\n      ", $language)}</p>
    {/if}
    <fieldset class="flex flex-col gap-2" {disabled}>
      <legend class="sr-only">{t("Candidati trovati", $language)}</legend>
      {#each $manualImport.candidates as candidate (candidate.path)}
        <label
          class="flex cursor-pointer items-start gap-3 rounded-lg bg-white/5 p-3 has-checked:bg-white/10 light:bg-zinc-100"
        >
          <input
            type="radio"
            name={group}
            class="mt-1"
            checked={$manualImport.selectedPath === candidate.path}
            onchange={() => chooseCandidate(candidate.path)}
          />
          <span class="min-w-0 flex-1">
            <span class="block break-all text-sm text-zinc-100 light:text-zinc-900">
              {candidate.path}
            </span>
            <span class="mt-1 flex flex-wrap items-center gap-2 text-xs text-zinc-500">
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
