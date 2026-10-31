<script lang="ts">
  import { untrack } from "svelte";
  import { t, language } from "../../i18n";
  import Button from "../../components/ui/Button.svelte";
  import Dialog from "../../components/ui/Dialog.svelte";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import TextField from "../../components/ui/TextField.svelte";
  import type { ModProfile } from "../../services/modding";
  import { toMessage } from "../../utils/errors";
  import { describeModdingError, profileNameError } from "./modding-model";

  let {
    open,
    title,
    confirmLabel,
    initialName = "",
    profiles,
    exceptId = null,
    onClose,
    onSubmit,
  }: {
    open: boolean;
    title: string;
    confirmLabel: string;
    initialName?: string;
    profiles: readonly ModProfile[];
    exceptId?: string | null;
    onClose: () => void;
    onSubmit: (name: string) => Promise<void>;
  } = $props();

  let name = $state("");
  let touched = $state(false);
  let pending = $state(false);
  let error = $state<string | null>(null);

  const validation = $derived(profileNameError(name, profiles, $language, exceptId));

  $effect(() => {
    if (!open) return;
    name = untrack(() => initialName);
    touched = false;
    error = null;
  });

  async function submit(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    touched = true;
    if (validation !== null || pending) return;
    pending = true;
    error = null;
    try {
      await onSubmit(name.trim());
      onClose();
    } catch (cause) {
      error = describeModdingError(toMessage(cause), $language);
    } finally {
      pending = false;
    }
  }
</script>

<Dialog {open} {title} dismissible={!pending} {onClose}>
  {#snippet children(close)}
    <form class="flex flex-col gap-5" onsubmit={(event) => void submit(event)}>
      <TextField
        id="mod-profile-name"
        label={t("Nome del profilo", $language)}
        bind:value={name}
        oninput={() => (touched = true)}
        disabled={pending}
        hint={touched && validation !== null ? validation : t("Al massimo 64 caratteri.", $language)}
      />
      {#if error !== null}<ErrorBanner message={error} />{/if}
      <div class="flex justify-end gap-2">
        <Button label={t("Annulla", $language)} variant="secondary" disabled={pending} onClick={close} />
        <Button type="submit" label={confirmLabel} disabled={pending || validation !== null} />
      </div>
    </form>
  {/snippet}
</Dialog>
