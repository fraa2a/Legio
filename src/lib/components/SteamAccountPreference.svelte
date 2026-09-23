<script lang="ts">
  import type { Game } from "../services/local-state";
  import { checkGameSteamAccount, listSavedSteamAccounts, setGameSteamAccountPreference, type SavedSteamAccounts, type SteamAccountCheck } from "../services/steam-accounts";

  let { game, onSaved }: { game: Game; onSaved: (game: Game) => void } = $props();
  let enabled = $state(false);
  let selectedId = $state("");
  let saved = $state<SavedSteamAccounts | null>(null);
  let check = $state<SteamAccountCheck | null>(null);
  let busy = $state(false);
  let error = $state<string | null>(null);
  let message = $state<string | null>(null);

  const selectedMissing = $derived(Boolean(enabled && selectedId && saved && !saved.accounts.some((account) => account.steamId === selectedId)));

  $effect(() => {
    enabled = game.steamAccountId !== null;
    selectedId = game.steamAccountId ?? "";
  });

  function labelFor(steamId: string, displayName: string): string {
    if (!saved || saved.accounts.filter((account) => account.displayName === displayName).length < 2) return displayName;
    return `${displayName} (${steamId})`;
  }

  async function loadAccounts() {
    busy = true;
    error = null;
    try {
      saved = await listSavedSteamAccounts();
      if (enabled && !selectedId) selectedId = saved.accounts[0]?.steamId ?? "";
    } catch (reason) {
      error = String(reason);
    } finally {
      busy = false;
    }
  }

  async function savePreference() {
    busy = true;
    error = null;
    message = null;
    try {
      if (enabled && !saved?.accounts.some((account) => account.steamId === selectedId)) {
        throw new Error("Select a saved Steam account first.");
      }
      const updated = await setGameSteamAccountPreference(game.id, enabled ? selectedId : null);
      onSaved(updated);
      check = null;
      message = "Steam account preference saved.";
    } catch (reason) {
      error = String(reason);
    } finally {
      busy = false;
    }
  }

  async function verifyPreference() {
    busy = true;
    error = null;
    try {
      check = await checkGameSteamAccount(game.id);
    } catch (reason) {
      error = String(reason);
    } finally {
      busy = false;
    }
  }

  function toggleOverride(checked: boolean) {
    enabled = checked;
    if (checked && !selectedId) selectedId = saved?.accounts[0]?.steamId ?? "";
  }

  $effect(() => { void loadAccounts(); });
</script>

<fieldset class="mt-4 rounded border border-slate-700 p-3" disabled={busy || !game.steamAppId}>
  <legend class="px-1 text-sm font-semibold">Steam account override</legend>
  <p class="mb-3 text-xs text-slate-400">When Steam is using a different or unknown account, launching asks before switching accounts and may close Steam and running Steam games.</p>
  <label class="flex items-center gap-2 text-sm">
    <input type="checkbox" checked={enabled} onchange={(event) => toggleOverride(event.currentTarget.checked)} disabled={!enabled && !saved?.accounts.length} /> Use a saved Steam account for this game
  </label>
  {#if enabled}
    <label class="mt-3 block text-sm">
      Saved account
      <select class="mt-1 block w-full rounded border border-slate-600 bg-slate-950 px-3 py-2" bind:value={selectedId}>
        {#if selectedMissing}
          <option value={selectedId} disabled>Previously selected account is no longer saved</option>
        {/if}
        {#each saved?.accounts ?? [] as account (account.steamId)}
          <option value={account.steamId}>{labelFor(account.steamId, account.displayName)}</option>
        {/each}
      </select>
    </label>
  {/if}
  <div class="mt-3 flex flex-wrap gap-3">
    <button class="rounded border border-slate-600 px-3 py-2 text-sm" type="button" onclick={loadAccounts}>Refresh accounts</button>
    <button class="rounded bg-amber-400 px-3 py-2 text-sm font-semibold text-slate-950" type="button" onclick={savePreference}>Save account override</button>
    <button class="rounded border border-slate-600 px-3 py-2 text-sm" type="button" onclick={verifyPreference}>Check saved preference</button>
  </div>
</fieldset>
{#if !game.steamAppId}<p class="mt-2 text-xs text-slate-400">Set and save a Steam App ID before selecting an account.</p>{/if}
{#if saved?.diagnostics.length}<ul class="mt-2 list-disc pl-5 text-xs text-amber-300">{#each saved.diagnostics as diagnostic, index (index)}<li>{diagnostic}</li>{/each}</ul>{/if}
{#if saved && !saved.accounts.length}<p class="mt-2 text-xs text-amber-300">No saved Steam accounts found. Sign in to Steam and refresh the account list before enabling an override.</p>{/if}
{#if selectedMissing}<p class="mt-2 text-xs text-amber-300">The selected account is no longer saved locally. Choose another account or clear the override.</p>{/if}
{#if check}<p class="mt-2 text-xs" role="status">Account check: {check.status}. {check.message ?? ""}</p>{/if}
{#if message}<p class="mt-2 text-xs" role="status">{message}</p>{/if}
{#if error}<p class="mt-2 break-words text-xs text-red-300" role="alert">{error}</p>{/if}
