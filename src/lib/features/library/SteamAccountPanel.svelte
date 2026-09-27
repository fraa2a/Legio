<script lang="ts">
  import { untrack } from "svelte";
  import type { Game } from "../../services/local-state";
  import { toMessage } from "../../utils/errors";
  import {
    ensureSavedSteamAccounts,
    saveGameSteamAccount,
    savedSteamAccounts,
  } from "../../stores/steam-accounts";
  import Button from "../../components/ui/Button.svelte";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import Panel from "../../components/ui/Panel.svelte";
  import SelectField from "../../components/ui/SelectField.svelte";
  import StateBlock from "../../components/ui/StateBlock.svelte";

  let { game }: { game: Game } = $props();

  const noAccount = "";

  const accounts = $derived($savedSteamAccounts.data.accounts);
  const loading = $derived($savedSteamAccounts.status === "loading");
  const options = $derived.by(() => {
    const options = [{ value: noAccount, label: "Nessun account" }];
    if (game.steamAccountId !== null && !accounts.some((account) => account.steamId === game.steamAccountId)) {
      options.push({ value: game.steamAccountId, label: `${game.steamAccountId} (non più salvato)` });
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

  const selectedSteamId = $derived(selection === noAccount ? null : selection);
  const changed = $derived(selectedSteamId !== game.steamAccountId);

  $effect(() => ensureSavedSteamAccounts);

  function refresh(): void {
    actionError = null;
    void savedSteamAccounts.load();
  }

  async function save(): Promise<void> {
    actionError = null;
    pending = true;
    try {
      await saveGameSteamAccount(game.id, selectedSteamId);
    } catch (error) {
      actionError = toMessage(error);
    } finally {
      pending = false;
    }
  }
</script>

<Panel title="Account Steam">
  {#snippet actions()}
    <Button label="Aggiorna" variant="secondary" disabled={loading} onClick={refresh} />
  {/snippet}

  <StateBlock
    status={$savedSteamAccounts.status}
    hasData={accounts.length > 0}
    loadingMessage="Ricerca degli account Steam salvati..."
    emptyMessage="Nessun account Steam salvato trovato su questo dispositivo."
    error={$savedSteamAccounts.error}
    onRetry={refresh}
  />

  <p class="text-sm text-zinc-400 light:text-zinc-600">
    L'account scelto viene applicato al prossimo avvio. Steam viene chiuso e riavviato quando serve
    per passare all'account salvato.
  </p>

  <SelectField
    id="steam-account"
    label="Account per l'avvio"
    bind:value={selection}
    {options}
    disabled={pending || loading}
  />

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

  <div class="flex justify-end">
    <Button
      label={pending ? "Salvataggio..." : "Salva account"}
      disabled={pending || !changed}
      onClick={() => void save()}
    />
  </div>
</Panel>
