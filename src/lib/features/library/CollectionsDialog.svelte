<script lang="ts">
  import { t, language } from "../../i18n";
  import Button from "../../components/ui/Button.svelte";
  import ConfirmDialog from "../../components/ui/ConfirmDialog.svelte";
  import Dialog from "../../components/ui/Dialog.svelte";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import Icon from "../../components/ui/Icon.svelte";
  import IconButton from "../../components/ui/IconButton.svelte";
  import TextField from "../../components/ui/TextField.svelte";
  import { maxCollectionNameLength, type Collection } from "../../services/library-organization";
  import { addCollection, libraryOrganization, moveCollection, removeCollection, renameExistingCollection } from "../../stores/library-organization";
  import { toMessage } from "../../utils/errors";

  let { onClose }: { onClose: () => void } = $props();

  let pending = $state(false);
  let error = $state<string | null>(null);
  let newName = $state("");
  let editingId = $state<string | null>(null);
  let editingName = $state("");
  let deleting = $state<Collection | null>(null);
  let deleteOpen = $state(false);
  const collections = $derived($libraryOrganization.data.collections);

  async function run(task: () => Promise<void>): Promise<boolean> {
    pending = true;
    error = null;
    try {
      await task();
      return true;
    } catch (cause) {
      error = toMessage(cause);
      return false;
    } finally {
      pending = false;
    }
  }

  function startRename(collection: Collection): void {
    editingId = collection.id;
    editingName = collection.name;
  }

  async function saveRename(id: string): Promise<void> {
    if (await run(() => renameExistingCollection(id, editingName))) editingId = null;
  }

  async function create(): Promise<void> {
    if (await run(async () => { await addCollection(newName); })) newName = "";
  }

  async function confirmDelete(): Promise<void> {
    const target = deleting;
    if (target === null) return;
    if (await run(() => removeCollection(target.id))) deleteOpen = false;
  }
</script>

<Dialog open title={t("Raccolte", $language)} dismissible={!pending} {onClose}>
  {#snippet children(dismiss)}
    {#if collections.length === 0}
      <p class="text-sm text-legio-fg-muted">{t("Nessuna raccolta. Creane una qui sotto.", $language)}</p>
    {:else}
      <ul class="flex flex-col gap-2" aria-label={t("Raccolte", $language)}>
        {#each collections as collection, index (collection.id)}
          <li class="flex items-center gap-2 rounded-control bg-legio-fill p-1 pl-3">
            {#if editingId === collection.id}
              <form class="flex min-w-0 flex-1 items-center gap-2" onsubmit={(event) => { event.preventDefault(); void saveRename(collection.id); }}>
                <TextField id="rename-collection" class="min-w-0 flex-1" label={t("Nome della raccolta", $language)} labelHidden bind:value={editingName} maxlength={maxCollectionNameLength} disabled={pending} />
                <Button type="submit" size="sm" label={t("Salva", $language)} disabled={pending || editingName.trim() === ""} />
                <Button size="sm" variant="ghost" label={t("Annulla", $language)} disabled={pending} onClick={() => (editingId = null)} />
              </form>
            {:else}
              <span class="min-w-0 flex-1 truncate text-sm text-legio-fg">{collection.name}</span>
              <span class="shrink-0 text-xs text-legio-fg-muted tabular-nums">{collection.gameIds.length}</span>
              <IconButton label={t("Sposta {0} in alto", $language, [collection.name])} disabled={pending || index === 0} onClick={() => void run(() => moveCollection(collection.id, -1))}>
                <Icon name="previous" class="rotate-90" />
              </IconButton>
              <IconButton label={t("Sposta {0} in basso", $language, [collection.name])} disabled={pending || index === collections.length - 1} onClick={() => void run(() => moveCollection(collection.id, 1))}>
                <Icon name="next" class="rotate-90" />
              </IconButton>
              <IconButton label={t("Rinomina {0}", $language, [collection.name])} disabled={pending} onClick={() => startRename(collection)}>
                <Icon name="pencil" />
              </IconButton>
              <IconButton label={t("Elimina {0}", $language, [collection.name])} variant="danger" disabled={pending} onClick={() => { deleting = collection; deleteOpen = true; }}>
                <Icon name="trash" />
              </IconButton>
            {/if}
          </li>
        {/each}
      </ul>
    {/if}
    <form class="flex items-end gap-2" onsubmit={(event) => { event.preventDefault(); if (newName.trim()) void create(); }}>
      <TextField id="new-collection" class="min-w-0 flex-1" label={t("Nuova raccolta", $language)} bind:value={newName} maxlength={maxCollectionNameLength} disabled={pending} />
      <Button type="submit" label={t("Crea", $language)} variant="secondary" disabled={pending || newName.trim() === ""} />
    </form>
    {#if error !== null && !deleteOpen}<ErrorBanner message={error} />{/if}
    <div class="flex justify-end">
      <Button label={t("Fine", $language)} disabled={pending} onClick={dismiss} />
    </div>
  {/snippet}
</Dialog>

{#if deleting !== null}
  <ConfirmDialog
    open={deleteOpen}
    title={t("Eliminare la raccolta?", $language)}
    message={t("La raccolta {0} verrà eliminata. I giochi restano in libreria.", $language, [deleting.name])}
    confirmLabel={t("Elimina", $language)}
    tone="danger"
    {pending}
    {error}
    onConfirm={() => void confirmDelete()}
    onClose={() => { deleteOpen = false; error = null; }}
  />
{/if}
