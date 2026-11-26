<script lang="ts">
  import { t, language } from "../../i18n";
  import { appInfo } from "../../stores/app-info";
  import { installAppUpdate, updateState } from "../../services/app-updater";
  import Button from "../../components/ui/Button.svelte";
  import Callout from "../../components/ui/Callout.svelte";
  import Dialog from "../../components/ui/Dialog.svelte";
  import RenderedMarkdown from "../../components/ui/RenderedMarkdown.svelte";

  type Announcement = { version: string; notes: string | null };

  // Kept after closing so the content stays during the exit animation.
  let shown: Announcement | null = $state(null);
  let open = $state(false);
  let dismissedVersion: string | null = null;

  const canInstall = $derived($appInfo.data.platform !== "linux" && !$updateState.aur);

  // A re-check clears the version for a moment, so an open dialog is never closed by it.
  $effect(() => {
    const version = $updateState.version;
    if (!version || open || version === dismissedVersion) return;
    if ($appInfo.data.platform === "linux" && $updateState.nativeNotified) return;
    shown = { version, notes: $updateState.notes };
    open = true;
  });

  function close(): void {
    dismissedVersion = shown?.version ?? null;
    open = false;
  }
</script>

<Dialog {open} title={t("Aggiornamento disponibile", $language)} dismissible={!$updateState.installing} onClose={close}>
  {#snippet children(dismiss)}
    <p class="text-sm text-legio-fg">{t("Legio {0} disponibile.", $language, [shown?.version ?? ""])}{#if $updateState.aur} {t("Aggiorna tramite AUR.", $language)}{/if}</p>
    {#if shown?.notes}
      <RenderedMarkdown markdown={shown.notes} class="max-h-[40vh] overflow-y-auto rounded-control bg-legio-fill p-3 text-legio-fg" />
    {/if}
    {#if $updateState.error}<Callout tone="warning" variant="inline" message={$updateState.error} />{/if}
    <div class="flex justify-end gap-2">
      <Button label={t("Più tardi", $language)} variant="secondary" disabled={$updateState.installing} onClick={dismiss} />
      {#if canInstall}
        <Button label={$updateState.installing ? t("Installazione...", $language) : t("Aggiorna ora", $language)} disabled={$updateState.installing} onClick={() => void installAppUpdate()} />
      {/if}
    </div>
  {/snippet}
</Dialog>
