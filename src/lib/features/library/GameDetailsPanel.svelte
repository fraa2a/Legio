<script lang="ts">
  import { t, language } from "../../i18n";
  import { onDestroy } from "svelte";
  import type { Game } from "../../services/local-state";
  import { toMessage } from "../../utils/errors";
  import { deleteGame, saveGame } from "../../stores/games";
  import { detectManualGameSteamAppId } from "../../stores/manual-import";
  import Button from "../../components/ui/Button.svelte";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import Panel from "../../components/ui/Panel.svelte";
  import TextField from "../../components/ui/TextField.svelte";
  import ResetSetting from "../../components/ui/ResetSetting.svelte";
  import type { SteamIdentityCandidate, SteamIdentificationResult } from "../../services/manual-import";

  let { game, onRemoved, section = "general" }: {
    game: Game;
    onRemoved: () => void;
    section?: "general" | "danger";
  } = $props();

  let nameDraft = $state<string | null>(null);
  let pending = $state(false);
  let confirmingRemove = $state(false);
  let actionError = $state<string | null>(null);
  let identityPending = $state(false);
  let identityError = $state<string | null>(null);
  let identityResult = $state<SteamIdentificationResult | null>(null);
  let failedName: string | null = null;

  const nameValue = $derived(nameDraft ?? game.nameOverride ?? game.name);
  const trimmedName = $derived(nameValue.trim());
  const nameChanged = $derived(nameDraft !== null && trimmedName !== (game.nameOverride ?? game.name).trim());
  const canResetName = $derived(game.automaticName !== null && (game.nameOverride !== null || nameChanged));

  $effect(() => {
    if (!nameChanged || trimmedName.length === 0 || pending || trimmedName === failedName) return;
    const timer = setTimeout(() => void saveName(trimmedName), 550);
    return () => clearTimeout(timer);
  });

  onDestroy(() => { if (nameChanged && trimmedName.length > 0 && !pending && trimmedName !== failedName) void saveName(trimmedName); });

  function handleNameInput(value: string): void {
    nameDraft = value;
  }

  async function saveName(override: string | null): Promise<void> {
    actionError = null;
    pending = true;
    try {
      await saveGame({
        id: game.id,
        steamAppId: game.steamAppId,
        automaticName: game.automaticName,
        nameOverride: override,
      });
      nameDraft = null;
      failedName = null;
    } catch (error) {
      failedName = override;
      actionError = toMessage(error);
    } finally {
      pending = false;
    }
  }

  async function remove(): Promise<void> {
    actionError = null;
    pending = true;
    try {
      await deleteGame(game.id);
      confirmingRemove = false;
      onRemoved();
    } catch (error) {
      actionError = toMessage(error);
    } finally {
      pending = false;
    }
  }

  async function detectIdentity(): Promise<void> {
    identityError = null;
    identityPending = true;
    try {
      identityResult = await detectManualGameSteamAppId(game.id);
    } catch (error) {
      identityError = toMessage(error);
    } finally {
      identityPending = false;
    }
  }

  async function linkCandidate(candidate: SteamIdentityCandidate): Promise<void> {
    identityError = null;
    identityPending = true;
    try {
      const updated = await saveGame({
        id: game.id,
        steamAppId: candidate.steamAppId,
        automaticName: candidate.name,
        nameOverride: game.nameOverride,
      });
      identityResult = {
        game: updated,
        status: "matched",
        candidates: [candidate],
        message: null,
      };
    } catch (error) {
      identityError = toMessage(error);
    } finally {
      identityPending = false;
    }
  }

  const identityMessage = $derived.by(() => {
    if (identityResult === null) return null;
    switch (identityResult.status) {
      case "matched":
        return t("App ID Steam rilevato: {0} ({1}).", $language, [identityResult.game.automaticName, identityResult.game.steamAppId]);
      case "ambiguous":
        return t("Trovate più corrispondenze con lo stesso nome. Scegli quella corretta.", $language);
      case "unavailable":
        return identityResult.message ?? t("Rilevazione Steam non disponibile.", $language);
      case "no_match":
        return t("Nessuna corrispondenza esatta nel catalogo Steam.", $language);
      default:
        return null;
    }
  });
</script>

{#if section === "general"}
<Panel title={t("Impostazioni", $language)}>
  <div class="flex items-end gap-2"><div class="min-w-0 flex-1"><TextField
    id="game-name"
    label={t("Nome visualizzato", $language)}
    value={nameValue}
    oninput={handleNameInput}
    disabled={pending}
    hint={game.automaticName !== null
      ? t("Nome automatico: {0}", $language, [game.automaticName])
      : t("Nessun nome automatico disponibile per questa voce.", $language)}
  /></div>
    {#if canResetName}<ResetSetting label={t("Ripristina nome automatico", $language)} disabled={pending} onClick={() => { nameDraft = null; if (game.nameOverride !== null) void saveName(null); }} />{/if}
  </div>
  {#if actionError !== null}<ErrorBanner message={actionError} />{/if}
  {#if pending}<p class="text-xs text-zinc-400" role="status">{t("Salvataggio...", $language)}</p>{/if}
</Panel>

{#if game.steamInstallPath === null && game.executablePath !== null}
  <Panel title={t("Identità Steam", $language)}>
    {#if game.steamAppId !== null}
      <p class="text-sm text-zinc-300 light:text-zinc-700">
        {game.automaticName ?? game.name} <span class="text-zinc-500">({game.steamAppId})</span>
      </p>
    {:else}
      <p class="text-sm text-zinc-400 light:text-zinc-600">{t("\n        Legio confronta il nome dell'eseguibile e della cartella con il catalogo Steam.\n      ", $language)}</p>
      <div>
        <Button
          label={identityPending ? t("Ricerca in corso...", $language) : t("Rileva Steam App ID", $language)}
          variant="secondary"
          disabled={identityPending}
          onClick={() => void detectIdentity()}
        />
      </div>
    {/if}
    {#if identityError !== null}
      <ErrorBanner message={identityError} onRetry={() => void detectIdentity()} />
    {/if}
    {#if identityMessage !== null}
      <p class="text-sm text-zinc-300 light:text-zinc-700" role="status">{identityMessage}</p>
    {/if}
    {#if identityResult?.status === "ambiguous"}
      <div class="flex flex-col gap-2">
        {#each identityResult.candidates as candidate (candidate.steamAppId)}
          <div class="flex flex-wrap items-center justify-between gap-2 rounded-lg bg-white/5 p-3 light:bg-zinc-100">
            <span class="text-sm text-zinc-100 light:text-zinc-900">
              {candidate.name} <span class="text-zinc-500">({candidate.steamAppId})</span>
            </span>
            <Button
              label={t("Collega", $language)}
              variant="secondary"
              disabled={identityPending}
              onClick={() => void linkCandidate(candidate)}
            />
          </div>
        {/each}
      </div>
    {/if}
  </Panel>
{/if}

{:else}
<Panel title={t("Rimuovi dalla libreria", $language)}>
  {#if actionError !== null}
    <ErrorBanner message={actionError} />
  {/if}
  {#if confirmingRemove}
    <p class="text-sm text-zinc-300 light:text-zinc-700">
      {game.name}{t(" verrà rimosso dalla libreria. I file installati non vengono eliminati.\n    ", $language)}</p>
    <div class="flex flex-wrap gap-2">
      <Button label={t("Annulla", $language)} variant="secondary" disabled={pending} onClick={() => (confirmingRemove = false)} />
      <Button label={t("Rimuovi", $language)} variant="danger" disabled={pending} onClick={() => void remove()} />
    </div>
  {:else}
    <p class="text-sm text-zinc-400 light:text-zinc-600">{t("\n      Rimuovi questa voce senza toccare i file del gioco.\n    ", $language)}</p>
    <div>
      <Button label={t("Rimuovi gioco", $language)} variant="danger" onClick={() => (confirmingRemove = true)} />
    </div>
  {/if}
</Panel>
{/if}
