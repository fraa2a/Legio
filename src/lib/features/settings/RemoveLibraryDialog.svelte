<script lang="ts">
  import { t, language } from "../../i18n";
  import type { GameLibrary } from "../../services/game-libraries";
  import { gameLibraries, removeLibrary, type LibraryRemoval } from "../../stores/game-libraries";
  import { showToast } from "../../stores/toast";
  import { toMessage } from "../../utils/errors";
  import Dialog from "../../components/ui/Dialog.svelte";
  import Button from "../../components/ui/Button.svelte";
  import Callout from "../../components/ui/Callout.svelte";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import ProgressBar from "../../components/ui/ProgressBar.svelte";
  import SegmentedControl from "../../components/ui/SegmentedControl.svelte";
  import LibraryPickerDialog from "../library/LibraryPickerDialog.svelte";

  let { library, onClose }: { library: GameLibrary; onClose: () => void } = $props();

  let mode = $state<"move" | "uninstall">("move");
  let pickerOpen = $state(false);
  let pending = $state(false);
  let error = $state<string | null>(null);
  let done = $state(0);
  let current = $state<string | null>(null);
  // Follows the refreshed list so games already moved or uninstalled drop out after a partial failure.
  const shown = $derived($gameLibraries.data.find(entry => entry.id === library.id) ?? library);
  const total = $derived(shown.games.length);

  async function run(removal: LibraryRemoval): Promise<void> {
    pickerOpen = false;
    pending = true;
    error = null;
    try {
      const outcome = await removeLibrary(shown, removal, (count, name) => { done = count; current = name; });
      for (const warning of outcome.warnings) showToast(warning, "warning");
      if (outcome.failure !== null) {
        error = t("Interrotto su {0}: {1}. La libreria resta nell'elenco e i giochi successivi non sono stati modificati.", $language, [outcome.failure.game, outcome.failure.message]);
        return;
      }
      showToast(t("Libreria rimossa.", $language), "success");
      onClose();
    } catch (cause) { error = toMessage(cause); }
    finally { pending = false; current = null; }
  }

  function confirm(): void {
    if (total > 0 && mode === "move") pickerOpen = true;
    else void run({ kind: "uninstall" });
  }
</script>

<Dialog open title={t("Rimuovi libreria", $language)} dismissible={!pending} {onClose}>
  {#snippet children(close)}
    <p class="break-all rounded-lg bg-white/5 p-3 font-mono text-xs text-zinc-200 light:bg-zinc-900/5 light:text-zinc-800">{shown.root}</p>
    {#if total === 0}
      <p class="text-sm text-zinc-300 light:text-zinc-700">{t("La libreria verrà tolta dall'elenco. La cartella e il suo catalogo restano sul disco: potrai aggiungerla di nuovo in seguito.", $language)}</p>
    {:else}
      <p class="text-sm text-zinc-300 light:text-zinc-700">{total === 1 ? t("Questa libreria contiene 1 gioco. Scegli cosa farne prima di rimuoverla.", $language) : t("Questa libreria contiene {0} giochi. Scegli cosa farne prima di rimuoverla.", $language, [String(total)])}</p>
      <ul class="flex max-h-40 flex-col gap-1 overflow-y-auto rounded-lg bg-white/5 p-3 text-sm text-zinc-200 light:bg-zinc-900/5 light:text-zinc-800">
        {#each shown.games as game (game.id)}<li class="truncate">{game.name}</li>{/each}
      </ul>
      <SegmentedControl name="library-removal" label={t("Giochi installati", $language)} value={mode} disabled={pending}
        options={[{ value: "move", label: t("Sposta in un'altra libreria", $language) }, { value: "uninstall", label: t("Disinstalla", $language) }]}
        onChange={(value) => (mode = value === "uninstall" ? "uninstall" : "move")} />
      {#if mode === "uninstall"}
        <Callout tone="danger" message={t("Le cartelle dei giochi verranno eliminate definitivamente, insieme a salvataggi e mod al loro interno.", $language)} />
      {/if}
    {/if}
    {#if pending && current !== null}
      <div class="flex flex-col gap-1.5">
        <p class="truncate text-sm text-zinc-300 light:text-zinc-700">{mode === "move" ? t("Spostamento di {0} ({1}/{2})...", $language, [current, String(done + 1), String(total)]) : t("Disinstallazione di {0} ({1}/{2})...", $language, [current, String(done + 1), String(total)])}</p>
        <ProgressBar value={(done / total) * 100} label={t("Avanzamento rimozione libreria", $language)} tone="accent" />
      </div>
    {/if}
    {#if error !== null}<ErrorBanner message={error} />{/if}
    <div class="flex justify-end gap-2">
      <Button label={t("Annulla", $language)} variant="secondary" disabled={pending} onClick={close} />
      <Button
        label={pending ? t("Rimozione...", $language) : total === 0 ? t("Rimuovi libreria", $language) : mode === "move" ? t("Scegli destinazione...", $language) : t("Disinstalla e rimuovi", $language)}
        variant={total > 0 && mode === "uninstall" ? "danger" : "primary"}
        disabled={pending}
        onClick={confirm} />
    </div>
  {/snippet}
</Dialog>

<LibraryPickerDialog
  open={pickerOpen}
  title={t("Sposta i giochi in", $language)}
  confirmLabel={t("Sposta e rimuovi", $language)}
  excludeId={library.id}
  onSelect={(target) => void run({ kind: "move", target })}
  onClose={() => (pickerOpen = false)}
/>
