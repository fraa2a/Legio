<script lang="ts">
  import { onMount, untrack } from "svelte";
  import { t, language } from "../../i18n";
  import type { Game } from "../../services/local-state";
  import {
    exportSteamShortcuts,
    listSteamShortcutAccounts,
    previewSteamShortcuts,
    removeExportedSteamShortcuts,
    type SteamShortcutAccounts,
    type SteamShortcutExport,
    type SteamShortcutPreview,
  } from "../../services/steam-shortcuts";
  import { toMessage } from "../../utils/errors";
  import Button from "../../components/ui/Button.svelte";
  import Callout from "../../components/ui/Callout.svelte";
  import ConfirmDialog from "../../components/ui/ConfirmDialog.svelte";
  import Dialog from "../../components/ui/Dialog.svelte";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import SelectField from "../../components/ui/SelectField.svelte";
  import StateBlock from "../../components/ui/StateBlock.svelte";
  import Toggle from "../../components/ui/Toggle.svelte";
  import { previewCounts, withSelection } from "./steam-export";

  let { games, onClose }: { games: Game[]; onClose: () => void } = $props();

  let accounts = $state<SteamShortcutAccounts | null>(null);
  let loadError = $state<string | null>(null);
  let steamId = $state("");
  let selected = $state<string[]>(untrack(() => (games.length === 1 ? [games[0].id] : [])));
  let preview = $state<SteamShortcutPreview | null>(null);
  let previewError = $state<string | null>(null);
  let working = $state(false);
  let actionError = $state<string | null>(null);
  let result = $state<SteamShortcutExport | null>(null);
  let removed = $state<number | null>(null);
  let confirmingRemoval = $state(false);
  let previewRequest = 0;
  let refreshes = $state(0);

  const account = $derived(accounts?.accounts.find((item) => item.steamId === steamId) ?? null);
  const options = $derived((accounts?.accounts ?? []).map((item) => ({ value: item.steamId, label: `${item.displayName} · ${item.steamId}` })));
  const steamRunning = $derived(accounts?.steamRunning ?? false);
  const counts = $derived(preview === null ? null : previewCounts(preview));
  const ready = $derived(account !== null && account.error === null && selected.length > 0 && !steamRunning && !working);
  const status = $derived(accounts === null ? (loadError === null ? "loading" : "error") : accounts.accounts.length === 0 ? "empty" : "ready");

  onMount(() => { void loadAccounts(); });

  async function loadAccounts(): Promise<void> {
    loadError = null;
    try {
      const loaded = await listSteamShortcutAccounts();
      accounts = loaded;
      if (!loaded.accounts.some((item) => item.steamId === steamId)) steamId = (loaded.accounts.find((item) => item.error === null) ?? loaded.accounts[0])?.steamId ?? "";
    } catch (error) {
      loadError = toMessage(error);
    }
  }

  // The preview follows the account and the checklist.
  $effect(() => {
    const current = { refresh: refreshes, id: steamId, ids: [...selected], blocked: account === null || account.error !== null };
    if (current.blocked || current.ids.length === 0) {
      preview = null;
      previewError = null;
      return;
    }
    const request = ++previewRequest;
    const timer = setTimeout(async () => {
      try {
        const next = await previewSteamShortcuts(current.id, current.ids);
        if (request !== previewRequest) return;
        preview = next;
        previewError = null;
      } catch (error) {
        if (request !== previewRequest) return;
        preview = null;
        previewError = toMessage(error);
      }
    }, 200);
    return () => clearTimeout(timer);
  });

  async function run(action: () => Promise<void>): Promise<void> {
    working = true;
    actionError = null;
    result = null;
    removed = null;
    try {
      await action();
    } catch (error) {
      actionError = toMessage(error);
    } finally {
      working = false;
      await loadAccounts();
      refreshes += 1;
    }
  }

  const exportGames = () => run(async () => { result = await exportSteamShortcuts(steamId, selected); });

  async function removeAll(): Promise<void> {
    confirmingRemoval = false;
    await run(async () => { removed = (await removeExportedSteamShortcuts(steamId, null)).removed; });
  }
</script>

<Dialog open title={t("Aggiungi a Steam", $language)} {onClose}>
  {#snippet children(close)}
    <p class="text-sm text-legio-fg-muted">{t("I giochi compaiono in Steam come giochi non Steam e si avviano tramite Legio, quindi tempo di gioco e impostazioni restano quelli di Legio.", $language)}</p>

    <StateBlock
      {status}
      hasData={accounts !== null && accounts.accounts.length > 0}
      loadingMessage={t("Ricerca degli account Steam salvati...", $language)}
      emptyMessage={t("Nessun account Steam con dati locali trovato su questo dispositivo.", $language)}
      error={loadError}
      onRetry={() => void loadAccounts()}
    />

    {#if accounts !== null && accounts.accounts.length > 0}
      {#if accounts.accounts.length > 1}
        <SelectField id="steam-export-account" label={t("Account Steam", $language)} bind:value={steamId} {options} disabled={working} />
      {:else if account !== null}
        <p class="text-sm text-legio-fg">{t("Account Steam: {0}", $language, [account.displayName])}</p>
      {/if}

      {#if account?.error}<ErrorBanner message={account.error} />{/if}

      {#if steamRunning}
        <Callout tone="warning" message={t("Chiudi Steam prima di continuare: lo riscriverebbe all'uscita annullando la modifica.", $language)}>
          <Button label={t("Ricontrolla", $language)} variant="link" size="sm" disabled={working} onClick={() => void loadAccounts()} />
        </Callout>
      {/if}

      <div class="flex max-h-64 flex-col gap-2 overflow-y-auto rounded-card border border-legio-line-subtle p-4">
        {#each games as game (game.id)}
          <Toggle label={game.name} checked={selected.includes(game.id)} disabled={working} onChange={(checked) => { selected = withSelection(selected, game.id, checked); }} />
        {/each}
      </div>
      {#if games.length > 1}
        <div class="flex gap-2">
          <Button label={t("Seleziona tutti", $language)} variant="ghost" size="sm" disabled={working} onClick={() => { selected = games.map((game) => game.id); }} />
          <Button label={t("Deseleziona tutti", $language)} variant="ghost" size="sm" disabled={working || selected.length === 0} onClick={() => { selected = []; }} />
        </div>
      {/if}

      {#if counts !== null}
        <p class="text-sm text-legio-fg-muted" role="status">{t("{0} da aggiungere, {1} da aggiornare, {2} già presenti.", $language, [counts.add, counts.update, counts.unchanged])}</p>
      {/if}
      {#if previewError !== null}<ErrorBanner message={previewError} />{/if}
      {#if actionError !== null}<ErrorBanner message={actionError} />{/if}

      {#if result !== null}
        <Callout tone="success" message={t("Aggiunti {0}, aggiornati {1}. Riavvia Steam per vedere i giochi.", $language, [result.added, result.updated])} items={result.warnings} />
      {/if}
      {#if removed !== null}
        <Callout tone="success" message={t("Rimossi {0} collegamenti. Riavvia Steam per aggiornare la libreria.", $language, [removed])} />
      {/if}

      <p class="text-xs text-legio-fg-subtle">{t("Legio modifica il file dei collegamenti di Steam e ne conserva una copia di backup accanto al file.", $language)}</p>

      <div class="flex flex-wrap justify-between gap-2">
        {#if account !== null && account.legioShortcuts > 0}
          <Button label={t("Rimuovi i collegamenti di Legio", $language)} variant="ghost" disabled={working || steamRunning || account.error !== null} onClick={() => { confirmingRemoval = true; }} />
        {:else}<span></span>{/if}
        <div class="flex gap-2">
          <Button label={t("Chiudi", $language)} variant="secondary" disabled={working} onClick={close} />
          <Button label={working ? t("Esportazione...", $language) : t("Esporta in Steam", $language)} disabled={!ready} onClick={() => void exportGames()} />
        </div>
      </div>
    {/if}
  {/snippet}
</Dialog>

{#if confirmingRemoval && account !== null}
  <ConfirmDialog
    title={t("Rimuovi i collegamenti di Legio", $language)}
    message={t("Verranno rimossi da Steam solo i collegamenti creati da Legio ({0}). Gli altri restano al loro posto.", $language, [account.legioShortcuts])}
    confirmLabel={t("Rimuovi", $language)}
    tone="danger"
    onConfirm={() => void removeAll()}
    onClose={() => { confirmingRemoval = false; }}
  />
{/if}
