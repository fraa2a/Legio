<script lang="ts">
  import { onDestroy, untrack } from "svelte";
  import { t, language } from "../../i18n";
  import {
    deleteModConfigFile, listModConfigFiles, openModConfigFile, readModConfigFile, saveModConfigEntries, saveModConfigText,
    type ConfigFileEntry, type ModConfigFile,
  } from "../../services/mod-config";
  import { showToast } from "../../stores/toast";
  import { toMessage } from "../../utils/errors";
  import { formatBytes, formatDateTime } from "../../utils/format";
  import Badge from "../../components/ui/Badge.svelte";
  import Button from "../../components/ui/Button.svelte";
  import ConfirmDialog from "../../components/ui/ConfirmDialog.svelte";
  import EmptyState from "../../components/ui/EmptyState.svelte";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import Icon from "../../components/ui/Icon.svelte";
  import IconButton from "../../components/ui/IconButton.svelte";
  import SelectField from "../../components/ui/SelectField.svelte";
  import SegmentedControl from "../../components/ui/SegmentedControl.svelte";
  import Spinner from "../../components/ui/Spinner.svelte";
  import TextField, { textFieldClass } from "../../components/ui/TextField.svelte";
  import ModConfigEntry from "./ModConfigEntry.svelte";
  import { describeModdingError } from "./modding-model";
  import {
    collectEdits, configSortOptions, currentValue, fileListPageSize, filterConfigFiles, filterSections, findInvalidEntries,
    initialOpenSections, sectionChangeCount, setChange, splitConfigPath, textSizeError, type ConfigChanges, type ConfigSort,
  } from "./mod-config-model";

  let { gameId, profileId, onDirtyChange }: {
    gameId: string;
    profileId: string;
    onDirtyChange?: (dirty: boolean) => void;
  } = $props();

  type Mode = "form" | "raw";

  let files = $state<ConfigFileEntry[]>([]);
  let listLoading = $state(true);
  let listError = $state<string | null>(null);
  let query = $state("");
  let sort = $state<ConfigSort>("name-asc");
  let visibleFiles = $state(fileListPageSize);
  let selectedPath = $state<string | null>(null);
  let file = $state<ModConfigFile | null>(null);
  let fileLoading = $state(false);
  let fileError = $state<string | null>(null);
  let mode = $state<Mode>("form");
  let changes = $state<ConfigChanges>({});
  let rawText = $state("");
  let entryQuery = $state("");
  let openSections = $state<string[]>([]);
  let saving = $state(false);
  let saveError = $state<string | null>(null);
  let guarded = $state<(() => void) | null>(null);
  let deleteOpen = $state(false);
  let deleting = $state(false);
  let deleteError = $state<string | null>(null);
  let actionError = $state<string | null>(null);
  let fileRequest = 0;
  let listRequest = 0;
  let disposed = false;

  const filtered = $derived(filterConfigFiles(files, query, sort));
  const shown = $derived(filtered.slice(0, visibleFiles));
  const config = $derived(file?.bepinex ?? null);
  const effectiveMode = $derived<Mode>(config === null ? "raw" : mode);
  const sections = $derived(config === null ? [] : filterSections(config, entryQuery));
  const searching = $derived(entryQuery.trim().length > 0);
  const dirty = $derived(
    file === null ? false : effectiveMode === "raw" ? rawText !== file.text : Object.keys(changes).length > 0,
  );
  const sizeError = $derived(effectiveMode === "raw" && dirty ? textSizeError(rawText, $language) : null);
  const invalid = $derived(effectiveMode === "form" && config !== null && findInvalidEntries(config, changes) > 0);
  const modeOptions = $derived([
    { value: "form", label: t("Modulo", $language) },
    { value: "raw", label: t("Testo", $language) },
  ]);
  const pathParts = $derived(selectedPath === null ? null : splitConfigPath(selectedPath));

  $effect(() => {
    void gameId;
    void profileId;
    untrack(() => {
      listRequest += 1;
      fileRequest += 1;
      files = [];
      selectedPath = null;
      file = null;
      fileError = null;
      saveError = null;
      guarded = null;
      query = "";
      changes = {};
      actionError = null;
      void loadFiles(true);
    });
  });

  $effect(() => {
    onDirtyChange?.(dirty);
  });

  $effect(() => {
    void query;
    visibleFiles = fileListPageSize;
  });

  onDestroy(() => {
    disposed = true;
    onDirtyChange?.(false);
  });

  async function loadFiles(selectFirst: boolean): Promise<void> {
    const request = ++listRequest;
    listLoading = true;
    listError = null;
    try {
      const result = await listModConfigFiles(gameId, profileId);
      if (request !== listRequest) return;
      files = result;
      const first = filterConfigFiles(result, "")[0];
      if (selectFirst && first !== undefined) void openFile(first.file.relativePath);
    } catch (reason) {
      if (request === listRequest) listError = describeModdingError(toMessage(reason), $language);
    } finally {
      if (request === listRequest) listLoading = false;
    }
  }

  async function openFile(path: string): Promise<void> {
    const request = ++fileRequest;
    selectedPath = path;
    fileLoading = true;
    fileError = null;
    saveError = null;
    try {
      const result = await readModConfigFile(gameId, profileId, path);
      if (request !== fileRequest) return;
      file = result;
      rawText = result.text;
      changes = {};
      entryQuery = "";
      openSections = result.bepinex === null ? [] : initialOpenSections(result.bepinex);
    } catch (reason) {
      if (request !== fileRequest) return;
      file = null;
      fileError = describeModdingError(toMessage(reason), $language);
    } finally {
      if (request === fileRequest) fileLoading = false;
    }
  }

  // Runs the action now, or after the user agrees to drop the unsaved edits.
  function guard(action: () => void): void {
    if (dirty) guarded = action;
    else action();
  }

  function selectFile(path: string): void {
    if (path === selectedPath) return;
    guard(() => void openFile(path));
  }

  function selectMode(next: string): void {
    if (next === mode) return;
    guard(() => {
      mode = next as Mode;
      discard();
    });
  }

  function discard(): void {
    changes = {};
    if (file !== null) rawText = file.text;
    saveError = null;
  }

  function toggleSection(id: string): void {
    openSections = openSections.includes(id) ? openSections.filter((open) => open !== id) : [...openSections, id];
  }

  async function save(): Promise<void> {
    if (file === null || selectedPath === null || !dirty || saving || invalid || sizeError !== null) return;
    const path = selectedPath;
    saving = true;
    saveError = null;
    try {
      if (effectiveMode === "raw") await saveModConfigText(gameId, profileId, path, rawText);
      else if (config !== null) await saveModConfigEntries(gameId, profileId, path, collectEdits(config, changes));
      if (disposed) return;
      showToast(t("Configurazione salvata.", $language), "success");
      await Promise.all([openFile(path), refreshFiles()]);
    } catch (reason) {
      if (!disposed) saveError = describeModdingError(toMessage(reason), $language);
    } finally {
      saving = false;
    }
  }

  async function openExternally(): Promise<void> {
    if (selectedPath === null) return;
    actionError = null;
    try {
      await openModConfigFile(gameId, profileId, selectedPath);
    } catch (reason) {
      actionError = describeModdingError(toMessage(reason), $language);
    }
  }

  async function deleteSelected(): Promise<void> {
    if (selectedPath === null || deleting) return;
    deleting = true;
    deleteError = null;
    try {
      await deleteModConfigFile(gameId, profileId, selectedPath);
      if (disposed) return;
      fileRequest += 1;
      selectedPath = null;
      file = null;
      changes = {};
      deleteOpen = false;
      showToast(t("File eliminato.", $language), "success");
      await loadFiles(false);
    } catch (reason) {
      if (!disposed) deleteError = describeModdingError(toMessage(reason), $language);
    } finally {
      deleting = false;
    }
  }

  async function refreshFiles(): Promise<void> {
    try {
      const result = await listModConfigFiles(gameId, profileId);
      if (!disposed) files = result;
    } catch {
      // The list only feeds size and date labels, the saved file is already re-read.
    }
  }
</script>

<div class="flex h-full min-h-[26rem] min-w-0 flex-col gap-4 lg:flex-row">
  <aside class="flex max-h-80 shrink-0 flex-col gap-3 rounded-2xl legio-glass p-3 lg:max-h-none lg:w-72" aria-label={t("File di configurazione", $language)}>
    <TextField id="mod-config-filter" label={t("Filtra file", $language)} type="search" placeholder={t("Filtra file...", $language)} bind:value={query} />
    <SelectField id="mod-config-sort" label={t("Ordina file", $language)} labelHidden value={sort} options={configSortOptions($language)} onChange={(value) => (sort = value as ConfigSort)} />
    {#if listLoading && files.length === 0}
      <div class="flex items-center gap-3 p-3 text-sm text-zinc-300 light:text-zinc-700" role="status"><Spinner />{t("Caricamento...", $language)}</div>
    {:else if listError !== null}
      <ErrorBanner message={listError} onRetry={() => void loadFiles(selectedPath === null)} />
    {:else if files.length === 0}
      <p class="p-3 text-sm text-zinc-400 light:text-zinc-600">{t("Nessun file di configurazione.", $language)}</p>
    {:else if filtered.length === 0}
      <p class="p-3 text-sm text-zinc-400 light:text-zinc-600">{t("Nessun file corrisponde al filtro.", $language)}</p>
    {:else}
      <ul class="flex min-h-0 flex-1 flex-col gap-1 overflow-y-auto">
        {#each shown as item (item.file.relativePath)}
          <li class="[contain-intrinsic-size:auto_3.5rem] [content-visibility:auto]">
            <button
              type="button"
              aria-current={item.file.relativePath === selectedPath ? "true" : undefined}
              onclick={() => selectFile(item.file.relativePath)}
              class="flex w-full flex-col gap-0.5 rounded-lg px-3 py-2 text-left text-sm text-zinc-300 transition-colors hover:bg-white/10 aria-[current=true]:bg-white/15 aria-[current=true]:text-white light:text-zinc-700 light:hover:bg-zinc-900/10 light:aria-[current=true]:bg-zinc-900/10 light:aria-[current=true]:text-zinc-900"
            >
              <span class="truncate font-medium">{item.name}</span>
              {#if item.directory}<span class="truncate text-xs text-zinc-500">{item.directory}</span>{/if}
              <span class="truncate text-xs text-zinc-500">{formatBytes(item.file.size, $language)} &middot; {formatDateTime(item.file.modified, $language)}</span>
            </button>
          </li>
        {/each}
        {#if filtered.length > shown.length}
          <li class="p-1">
            <Button label={t("Mostra altri ({0})", $language, [filtered.length - shown.length])} variant="secondary" size="sm" class="w-full" onClick={() => (visibleFiles += fileListPageSize)} />
          </li>
        {/if}
      </ul>
    {/if}
  </aside>

  <section class="flex min-w-0 flex-1 flex-col rounded-2xl legio-glass" aria-label={t("Editor di configurazione", $language)}>
    {#if listLoading && files.length === 0}
      <div class="flex flex-1 items-center justify-center gap-3 text-sm text-zinc-300 light:text-zinc-700" role="status"><Spinner />{t("Caricamento...", $language)}</div>
    {:else if !listLoading && listError === null && files.length === 0}
      <div class="flex flex-1 items-center justify-center p-6">
        <EmptyState
          icon="file-code"
          title={t("Nessun file di configurazione", $language)}
          description={t("Avvia il gioco con questo profilo per generare i file di configurazione delle mod.", $language)}
        />
      </div>
    {:else if selectedPath === null || pathParts === null}
      <div class="flex flex-1 items-center justify-center p-6 text-sm text-zinc-400 light:text-zinc-600">{t("Seleziona un file da modificare.", $language)}</div>
    {:else}
      <header class="flex shrink-0 flex-wrap items-center gap-3 border-b border-white/10 px-4 py-3 light:border-zinc-900/10">
        <div class="min-w-0 flex-1">
          <p class="flex items-center gap-2 truncate text-sm font-semibold text-zinc-100 light:text-zinc-900">
            <span class="truncate">{pathParts.name}</span>
            {#if dirty}<Badge tone="info" title={t("Modifiche non salvate", $language)} />{/if}
          </p>
          {#if pathParts.directory}<p class="truncate text-xs text-zinc-500">{pathParts.directory}</p>{/if}
        </div>
        {#if config !== null}
          <SegmentedControl name="mod-config-mode" label={t("Modalità di modifica", $language)} labelHidden class="w-48" options={modeOptions} value={effectiveMode} disabled={saving} onChange={selectMode} />
        {/if}
        <IconButton
          label={t("Apri con l'editor di sistema", $language)}
          title={dirty ? t("Salva o annulla le modifiche prima di aprire il file", $language) : t("Apri con l'editor di sistema", $language)}
          disabled={saving || dirty}
          onClick={() => void openExternally()}
        >
          <Icon name="folder-open" size="size-4" />
        </IconButton>
        <IconButton variant="danger" label={t("Elimina file", $language)} title={t("Elimina file", $language)} disabled={saving} onClick={() => { deleteError = null; deleteOpen = true; }}>
          <Icon name="trash" size="size-4" />
        </IconButton>
        <Button label={t("Annulla modifiche", $language)} variant="secondary" size="sm" disabled={!dirty || saving} onClick={discard} />
        <Button label={saving ? t("Salvataggio...", $language) : t("Salva", $language)} size="sm" disabled={!dirty || saving || invalid || sizeError !== null} onClick={() => void save()} />
      </header>

      {#if actionError !== null}<div class="px-4 pt-4"><ErrorBanner message={actionError} /></div>{/if}
      {#if fileError !== null}
        <div class="p-4"><ErrorBanner message={fileError} onRetry={() => void openFile(selectedPath ?? "")} /></div>
      {:else if file === null || fileLoading}
        <div class="flex flex-1 items-center justify-center gap-3 text-sm text-zinc-300 light:text-zinc-700" role="status"><Spinner />{t("Caricamento...", $language)}</div>
      {:else}
        {#if saveError !== null}<div class="px-4 pt-4"><ErrorBanner message={saveError} /></div>{/if}
        {#if sizeError !== null}<div class="px-4 pt-4"><ErrorBanner message={sizeError} tone="warning" /></div>{/if}
        {#if effectiveMode === "raw"}
          <div class="flex min-h-0 flex-1 p-4">
            <textarea
              aria-label={t("Contenuto del file", $language)}
              spellcheck="false"
              wrap="off"
              value={rawText}
              disabled={saving}
              oninput={(event) => (rawText = event.currentTarget.value)}
              class="{textFieldClass} min-h-[18rem] flex-1 resize-none font-mono"
            ></textarea>
          </div>
        {:else if config !== null}
          <div class="flex min-h-0 flex-1 flex-col gap-3 p-4">
            <TextField id="mod-config-entry-filter" label={t("Cerca impostazione", $language)} type="search" placeholder={t("Cerca impostazione...", $language)} bind:value={entryQuery} />
            <div class="flex min-h-0 flex-1 flex-col gap-3 overflow-y-auto pr-1">
              {#each sections as section (section.id)}
                {@const changedCount = sectionChangeCount(section, changes)}
                {@const expanded = searching || openSections.includes(section.id)}
                <div class="flex flex-col gap-2">
                  <button
                    type="button"
                    aria-expanded={expanded}
                    onclick={() => toggleSection(section.id)}
                    class="flex w-full items-center gap-2 rounded-lg px-2 py-1.5 text-left text-sm font-semibold text-zinc-100 transition-colors hover:bg-white/10 light:text-zinc-900 light:hover:bg-zinc-900/10"
                  >
                    <span class="shrink-0 text-zinc-400 transition-transform motion-reduce:transition-none {expanded ? '' : '-rotate-90'}"><Icon name="chevron-down" size="size-4" /></span>
                    <span class="min-w-0 truncate">{section.name}</span>
                    <span class="text-xs font-normal text-zinc-500">{section.entries.length}</span>
                    {#if changedCount > 0}<Badge tone="info" title={t("{0} modificate", $language, [changedCount])} />{/if}
                  </button>
                  {#if expanded}
                    <div class="flex flex-col gap-2">
                      {#each section.entries as entry (entry.line)}
                        <ModConfigEntry
                          id={`mod-config-line-${entry.line}`}
                          {entry}
                          value={currentValue(section.name, entry, changes)}
                          onChange={(value) => (changes = setChange(changes, section.name, entry, value))}
                        />
                      {/each}
                    </div>
                  {/if}
                </div>
              {:else}
                <p class="p-3 text-sm text-zinc-400 light:text-zinc-600">{config.sections.length === 0 ? t("Il file non contiene impostazioni.", $language) : t("Nessuna impostazione corrisponde alla ricerca.", $language)}</p>
              {/each}
            </div>
          </div>
        {/if}
      {/if}
    {/if}
  </section>
</div>

<ConfirmDialog
  open={guarded !== null}
  title={t("Modifiche non salvate", $language)}
  message={t("Le modifiche a questo file andranno perse. Continuare?", $language)}
  confirmLabel={t("Scarta modifiche", $language)}
  cancelLabel={t("Continua a modificare", $language)}
  tone="danger"
  onConfirm={() => {
    const action = guarded;
    guarded = null;
    action?.();
  }}
  onClose={() => (guarded = null)}
/>

<ConfirmDialog
  open={deleteOpen}
  title={t("Elimina file", $language)}
  message={t("Eliminare il file {0}? Verrà rimosso dal profilo e non potrà essere ripristinato.", $language, [pathParts?.name ?? ""])}
  confirmLabel={deleting ? t("Eliminazione...", $language) : t("Elimina", $language)}
  tone="danger"
  pending={deleting}
  error={deleteError}
  onConfirm={() => void deleteSelected()}
  onClose={() => (deleteOpen = false)}
/>
