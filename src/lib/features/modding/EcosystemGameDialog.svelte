<script lang="ts">
  import { t, language } from "../../i18n";
  import Badge from "../../components/ui/Badge.svelte";
  import Button from "../../components/ui/Button.svelte";
  import Callout from "../../components/ui/Callout.svelte";
  import ConfirmDialog from "../../components/ui/ConfirmDialog.svelte";
  import Dialog from "../../components/ui/Dialog.svelte";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import Icon from "../../components/ui/Icon.svelte";
  import StateBlock from "../../components/ui/StateBlock.svelte";
  import TextField from "../../components/ui/TextField.svelte";
  import { ecosystemGames, loadOnce } from "../../stores/modding";
  import { toMessage } from "../../utils/errors";
  import { filterEcosystemGames } from "./modding-model";

  let {
    open,
    gameName,
    currentId,
    suggestedId = null,
    profileCount = 0,
    onClose,
    onSelect,
  }: {
    open: boolean;
    gameName: string;
    currentId: string | null;
    suggestedId?: string | null;
    profileCount?: number;
    onClose: () => void;
    onSelect: (ecosystemGameId: string) => Promise<void>;
  } = $props();

  let query = $state("");
  let chosen = $state<string | null>(null);
  let pending = $state(false);
  let error = $state<string | null>(null);
  let confirmingRebind = $state(false);

  const selectedId = $derived(chosen ?? currentId ?? suggestedId);
  const selectedGame = $derived($ecosystemGames.data.find((game) => game.id === selectedId) ?? null);
  const matches = $derived(filterEcosystemGames($ecosystemGames.data, query));
  // Profiles keep their mods when the game moves to another community, so the user confirms the switch.
  const needsConfirmation = $derived(currentId !== null && selectedId !== currentId && profileCount > 0);

  $effect(() => {
    if (!open) return;
    query = "";
    chosen = null;
    error = null;
    confirmingRebind = false;
    loadOnce(ecosystemGames);
  });

  function confirm(): void {
    if (selectedId === null) return;
    if (needsConfirmation) confirmingRebind = true;
    else void bind(selectedId);
  }

  async function bind(ecosystemGameId: string): Promise<void> {
    pending = true;
    error = null;
    try {
      await onSelect(ecosystemGameId);
      confirmingRebind = false;
      onClose();
    } catch (cause) {
      confirmingRebind = false;
      error = toMessage(cause);
    } finally {
      pending = false;
    }
  }
</script>

<Dialog {open} title={t("Gioco Thunderstore", $language)} dismissible={!pending} {onClose}>
  {#snippet children(close)}
    <div class="flex flex-col gap-4">
      <p class="text-sm text-zinc-400 light:text-zinc-600">
        {t("Scegli il gioco Thunderstore che corrisponde a {0}. Le mod e i profili useranno il suo catalogo.", $language, [gameName])}
      </p>
      <TextField id="ecosystem-game-search" type="search" label={t("Cerca", $language)} placeholder={t("Nome del gioco", $language)} bind:value={query} />
      <StateBlock
        status={$ecosystemGames.status}
        hasData={$ecosystemGames.data.length > 0}
        loadingMessage={t("Caricamento dei giochi Thunderstore...", $language)}
        error={$ecosystemGames.error}
        onRetry={() => void ecosystemGames.reload()}
      />
      {#if $ecosystemGames.data.length > 0}
        {#if matches.length === 0}
          <p class="py-6 text-center text-sm text-zinc-400 light:text-zinc-600">{t("Nessun gioco corrisponde alla ricerca.", $language)}</p>
        {:else}
          <div role="group" aria-label={t("Giochi Thunderstore", $language)} class="-mx-1 flex max-h-72 flex-col gap-0.5 overflow-y-auto px-1">
            {#each matches as game (game.id)}
              <button
                type="button"
                aria-pressed={game.id === selectedId}
                onclick={() => (chosen = game.id)}
                class="flex min-h-11 w-full items-center gap-3 rounded-lg px-3 py-2 text-left outline-none transition-colors hover:bg-white/10 focus-visible:bg-white/10 aria-pressed:bg-white/10 light:hover:bg-zinc-900/5 light:focus-visible:bg-zinc-900/5 light:aria-pressed:bg-zinc-900/10"
              >
                <span class="min-w-0 flex-1 truncate text-sm text-zinc-100 light:text-zinc-900">{game.displayName}</span>
                {#if !game.supported}<Badge tone="warning" title={t("Loader non supportato", $language)} />{/if}
                {#if game.id === selectedId}<Icon name="check" size="size-4 shrink-0 text-legio-success" />{/if}
              </button>
            {/each}
          </div>
        {/if}
      {/if}
      {#if selectedGame !== null && !selectedGame.supported}
        <Callout tone="warning" variant="inline" message={t("{0} usa un loader che Legio non supporta ancora: puoi collegarlo, ma non installare mod.", $language, [selectedGame.displayName])} />
      {/if}
      {#if error !== null}<ErrorBanner message={error} />{/if}
      <div class="flex justify-end gap-2">
        <Button label={t("Annulla", $language)} variant="secondary" disabled={pending} onClick={close} />
        <Button
          label={pending ? t("Collegamento...", $language) : t("Collega", $language)}
          disabled={pending || selectedId === null || selectedId === currentId}
          onClick={confirm}
        />
      </div>
    </div>
  {/snippet}
</Dialog>

<ConfirmDialog
  open={confirmingRebind}
  title={t("Cambiare gioco Thunderstore?", $language)}
  message={t("I profili di {0} restano collegati al gioco e conservano le mod già installate, ma aggiornamenti, dipendenze e regole di installazione useranno il catalogo di {1}. Le mod installate potrebbero non essere compatibili: in quel caso crea un nuovo profilo.", $language, [gameName, selectedGame?.displayName ?? ""])}
  confirmLabel={t("Cambia gioco", $language)}
  pending={pending}
  onConfirm={() => { if (selectedId !== null) void bind(selectedId); }}
  onClose={() => (confirmingRebind = false)}
/>
