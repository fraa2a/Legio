<script lang="ts">
  import { t, language } from "../../i18n";
  import { onDestroy, onMount, untrack } from "svelte";
  import type { Game } from "../../services/local-state";
  import { toMessage } from "../../utils/errors";
  import {
    ensureSavedSteamAccounts,
    saveGameSteamAccount,
    savedSteamAccounts,
  } from "../../stores/steam-accounts";
  import Button from "../../components/ui/Button.svelte";
  import ResetSetting from "../../components/ui/ResetSetting.svelte";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import Panel from "../../components/ui/Panel.svelte";
  import SelectField from "../../components/ui/SelectField.svelte";
  import StateBlock from "../../components/ui/StateBlock.svelte";

  let { game }: { game: Game } = $props();

  const noAccount = "";

  const accounts = $derived($savedSteamAccounts.data.accounts);
  const loading = $derived($savedSteamAccounts.status === "loading");
  const options = $derived.by(() => {
    const options = [{ value: noAccount, label: t("Nessun account", $language) }];
    if (game.steamAccountId !== null && !accounts.some((account) => account.steamId === game.steamAccountId)) {
      options.push({ value: game.steamAccountId, label: t("{0} (non più salvato)", $language, [game.steamAccountId]) });
    }
    for (const account of accounts) {
      options.push({ value: account.steamId, label: `${account.displayName} · ${account.steamId}` });
    }
    return options;
  });

  // The parent remounts this panel per game, so the stored account is read once.
  let selection = $state(untrack(() => game.steamAccountId ?? noAccount));
  let pending = $state(false);
  let actionError = $state<string | null>(null);
  let failedSelection: string | null = null;

  const selectedSteamId = $derived(selection === noAccount ? null : selection);
  const changed = $derived(selectedSteamId !== game.steamAccountId);

  onMount(ensureSavedSteamAccounts);

  $effect(() => {
    if (!changed || pending || loading || selection === failedSelection) return;
    const timer = setTimeout(() => void save(), 200);
    return () => clearTimeout(timer);
  });

  onDestroy(() => { if (changed && !pending && selection !== failedSelection) void save(); });

  function refresh(): void {
    actionError = null;
    void savedSteamAccounts.load();
  }

  async function save(): Promise<void> {
    actionError = null;
    pending = true;
    try {
      await saveGameSteamAccount(game.id, selectedSteamId);
      failedSelection = null;
    } catch (error) {
      failedSelection = selection;
      actionError = toMessage(error);
    } finally {
      pending = false;
    }
  }
</script>

<Panel title={t("Account Steam", $language)}>
  {#snippet actions()}
    <Button label={t("Aggiorna", $language)} variant="secondary" disabled={loading} onClick={refresh} />
  {/snippet}

  <StateBlock
    status={$savedSteamAccounts.status}
    hasData={accounts.length > 0}
    loadingMessage={t("Ricerca degli account Steam salvati...", $language)}
    emptyMessage={t("Nessun account Steam salvato trovato su questo dispositivo.", $language)}
    error={$savedSteamAccounts.error}
    onRetry={refresh}
  />

  <p class="text-sm text-zinc-400 light:text-zinc-600">{t("\n    L'account scelto viene applicato al prossimo avvio. Steam viene chiuso e riavviato quando serve\n    per passare all'account salvato.\n  ", $language)}</p>

  <div class="flex items-end gap-2"><div class="min-w-0 flex-1"><SelectField
    id="steam-account"
    label={t("Account per l'avvio", $language)}
    bind:value={selection}
    {options}
    disabled={pending || loading}
  /></div><ResetSetting label={t("Ripristina account Steam", $language)} disabled={pending || loading || selection === noAccount} onClick={() => { selection = noAccount; }} /></div>

  {#if $savedSteamAccounts.data.diagnostics.length > 0}
    <ul class="flex flex-col gap-1 text-xs text-amber-300 light:text-amber-800">
      {#each $savedSteamAccounts.data.diagnostics as diagnostic (diagnostic)}
        <li>{diagnostic}</li>
      {/each}
    </ul>
  {/if}

  {#if actionError !== null}
    <ErrorBanner message={actionError} />
  {/if}

  {#if pending}<p class="text-sm text-zinc-400" role="status">{t("Salvataggio...", $language)}</p>{/if}
</Panel>
