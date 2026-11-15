<script lang="ts">
  import { t, language } from "../../i18n";
  import type { GameLibrary } from "../../services/game-libraries";
  import { gameLibraries, hasRoomFor, pickAndAddLibrary } from "../../stores/game-libraries";
  import { openSettings } from "../../stores/navigation";
  import { showToast } from "../../stores/toast";
  import { formatBytes } from "../../utils/format";
  import { toMessage } from "../../utils/errors";
  import { formatLibraryDetails } from "./library-details";
  import Dialog from "../../components/ui/Dialog.svelte";
  import Button from "../../components/ui/Button.svelte";
  import Badge from "../../components/ui/Badge.svelte";
  import Icon from "../../components/ui/Icon.svelte";
  import Callout from "../../components/ui/Callout.svelte";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";

  let { open, title, confirmLabel, excludeId = null, requiredBytes = null, onSelect, onClose }: {
    open: boolean;
    title: string;
    confirmLabel: string;
    excludeId?: string | null;
    requiredBytes?: number | null;
    onSelect: (library: GameLibrary) => void;
    onClose: () => void;
  } = $props();

  let selectedId = $state<string | null>(null);
  let adding = $state(false);

  const choices = $derived($gameLibraries.data.filter(library => library.id !== excludeId));
  const selected = $derived(choices.find(library => library.id === selectedId && library.freeBytes !== null)
    ?? choices.find(library => library.isDefault && library.freeBytes !== null)
    ?? choices.find(library => library.freeBytes !== null)
    ?? null);
  const short = $derived(selected !== null && !hasRoomFor(selected, requiredBytes));

  $effect(() => {
    if (!open) return;
    selectedId = null;
    void gameLibraries.reload();
  });

  async function add(): Promise<void> {
    adding = true;
    try {
      const id = await pickAndAddLibrary();
      if (id !== null) selectedId = id;
    } catch (cause) { showToast(toMessage(cause), "error"); }
    finally { adding = false; }
  }

  function manage(): void {
    onClose();
    openSettings("libraries");
  }
</script>

<Dialog {open} {title} {onClose}>
  {#snippet children(close)}
    {#if requiredBytes !== null}
      <p class="text-sm text-legio-fg-muted">{t("Spazio richiesto: {0}", $language, [formatBytes(requiredBytes, $language)])}</p>
    {/if}
    {#if $gameLibraries.error !== null}<ErrorBanner message={$gameLibraries.error} onRetry={() => void gameLibraries.reload()} />{/if}
    <div role="radiogroup" aria-label={t("Librerie disponibili", $language)} class="flex max-h-72 flex-col gap-2 overflow-y-auto">
      {#each choices as library (library.id)}
        {@const checked = selected?.id === library.id}
        <button type="button" role="radio" aria-checked={checked} disabled={library.freeBytes === null} onclick={() => (selectedId = library.id)}
          class="flex items-center gap-3 rounded-lg border p-3 text-left transition-colors disabled:cursor-not-allowed disabled:opacity-50 {checked ? 'border-legio-line-strong bg-legio-fill-hover' : 'border-legio-line-subtle enabled:hover:bg-legio-fill'}">
          <span class="shrink-0 text-legio-fg-muted"><Icon name="folder" /></span>
          <span class="flex min-w-0 flex-1 flex-col gap-0.5">
            <span class="flex min-w-0 items-center gap-2">
              <span class="truncate text-sm font-medium text-legio-fg" title={library.root}>{library.root}</span>
              {#if library.isDefault}<Badge title={t("Predefinita", $language)} />{/if}
            </span>
            <span class="text-xs text-legio-fg-muted">{formatLibraryDetails(library, $language)}</span>
          </span>
          <span class="shrink-0 text-legio-fg {checked ? '' : 'invisible'}"><Icon name="check" size="size-4" /></span>
        </button>
      {:else}
        <p class="text-sm text-legio-fg-muted">{t("Nessun'altra libreria disponibile. Aggiungine una nuova.", $language)}</p>
      {/each}
    </div>
    <div class="flex flex-wrap items-center justify-between gap-2">
      <Button label={adding ? t("Aggiunta...", $language) : t("Aggiungi libreria...", $language)} variant="secondary" size="sm" disabled={adding} onClick={() => void add()}>
        <Icon name="plus" size="size-4" />
      </Button>
      <button type="button" class="rounded text-xs text-legio-fg-muted underline-offset-2 hover:text-legio-fg hover:underline" onclick={manage}>
        {t("Gestisci librerie", $language)}
      </button>
    </div>
    {#if short}<Callout tone="warning" variant="inline" message={t("Lo spazio libero potrebbe non bastare per questo gioco.", $language)} />{/if}
    <div class="flex justify-end gap-2">
      <Button label={t("Annulla", $language)} variant="secondary" onClick={close} />
      <Button label={confirmLabel} disabled={selected === null || adding} onClick={() => { if (selected !== null) onSelect(selected); }} />
    </div>
  {/snippet}
</Dialog>
