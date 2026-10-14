<script lang="ts">
  import { t, language } from "../../i18n";
  import { onDestroy } from "svelte";
  import type { Game } from "../../services/local-state";
  import { toMessage } from "../../utils/errors";
  import { deleteGame, saveGame } from "../../stores/games";
  import GameSourcePanel from "./GameSourcePanel.svelte";
  import SteamGameSearch from "./SteamGameSearch.svelte";
  import Button from "../../components/ui/Button.svelte";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import Panel from "../../components/ui/Panel.svelte";
  import TextField from "../../components/ui/TextField.svelte";
  import ResetSetting from "../../components/ui/ResetSetting.svelte";
  import type { CatalogGame } from "../../services/catalog";

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
  let failedName: string | null = null;

  const nameValue = $derived(nameDraft ?? game.nameOverride ?? game.name);
  const trimmedName = $derived(nameValue.trim());
  const nameChanged = $derived(nameDraft !== null && trimmedName !== (game.nameOverride ?? game.name).trim());
  const canResetName = $derived(game.automaticName !== null && (game.nameOverride !== null || nameChanged));

  $effect(() => {
    if (!nameChanged || trimmedName.length === 0 || pending || identityPending || trimmedName === failedName) return;
    const timer = setTimeout(() => void saveName(trimmedName), 550);
    return () => clearTimeout(timer);
  });

  onDestroy(() => { if (nameChanged && trimmedName.length > 0 && !pending && !identityPending && trimmedName !== failedName) void saveName(trimmedName); });

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

  async function linkCandidate(candidate: CatalogGame): Promise<boolean> {
    if (pending || identityPending) return false;
    identityError = null;
    identityPending = true;
    try {
      await saveGame({
        id: game.id,
        steamAppId: candidate.steamAppId,
        automaticName: candidate.name,
        nameOverride: nameDraft !== null ? trimmedName : game.nameOverride,
      });
      nameDraft = null;
      failedName = null;
      return true;
    } catch (error) {
      identityError = toMessage(error);
      return false;
    } finally {
      identityPending = false;
    }
  }
</script>

{#if section === "general"}
<Panel title={t("Impostazioni", $language)}>
  <div class="flex items-end gap-2"><div class="min-w-0 flex-1"><TextField
    id="game-name"
    label={t("Nome visualizzato", $language)}
    value={nameValue}
    oninput={handleNameInput}
    disabled={pending || identityPending}
    hint={game.automaticName !== null
      ? t("Nome automatico: {0}", $language, [game.automaticName])
      : t("Nessun nome automatico disponibile per questa voce.", $language)}
  /></div>
    {#if canResetName}<ResetSetting label={t("Ripristina nome automatico", $language)} disabled={pending || identityPending} onClick={() => { nameDraft = null; if (game.nameOverride !== null) void saveName(null); }} />{/if}
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
    {/if}
    <SteamGameSearch id="edit-steam-game-search" disabled={pending || identityPending} onSelect={linkCandidate} />
    {#if identityPending}<p class="text-xs text-zinc-400" role="status">{t("Salvataggio...", $language)}</p>{/if}
    {#if identityError !== null}<ErrorBanner message={identityError} />{/if}
  </Panel>
{/if}

<GameSourcePanel {game} />

{:else}
<Panel title={t("Rimuovi dalla libreria", $language)}>
  {#if actionError !== null}
    <ErrorBanner message={actionError} />
  {/if}
  {#if confirmingRemove}
    <p class="text-sm text-zinc-300 light:text-zinc-700">
      {game.name}{t(" verrà rimosso dalla libreria. I file installati non vengono eliminati.\n    ", $language)}</p>
    <div class="flex flex-wrap gap-2">
      <Button label={t("Annulla", $language)} variant="secondary" disabled={pending || identityPending} onClick={() => (confirmingRemove = false)} />
      <Button label={t("Rimuovi", $language)} variant="danger" disabled={pending || identityPending} onClick={() => void remove()} />
    </div>
  {:else}
    <p class="text-sm text-zinc-400 light:text-zinc-600">{t("\n      Rimuovi questa voce senza toccare i file del gioco.\n    ", $language)}</p>
    <div>
      <Button label={t("Rimuovi gioco", $language)} variant="danger" onClick={() => (confirmingRemove = true)} />
    </div>
  {/if}
</Panel>
{/if}
