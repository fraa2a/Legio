<script lang="ts">
  import { t, language } from "../../i18n";
  import Button from "../../components/ui/Button.svelte";
  import Dialog from "../../components/ui/Dialog.svelte";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import SelectField from "../../components/ui/SelectField.svelte";
  import TextField from "../../components/ui/TextField.svelte";
  import Toggle from "../../components/ui/Toggle.svelte";
  import { maxCollectionNameLength, type PlayStatus } from "../../services/library-organization";
  import { addCollection, libraryOrganization, setCollections, setPlayStatus } from "../../stores/library-organization";
  import { toMessage } from "../../utils/errors";
  import { collectionMembership } from "./library-organize";
  import { playStatusOptions } from "./library-labels";

  let { name, gameIds, playStatus, onClose }: { name: string; gameIds: string[]; playStatus: PlayStatus; onClose: () => void } = $props();

  let pending = $state(false);
  let error = $state<string | null>(null);
  let newName = $state("");
  const collections = $derived($libraryOrganization.data.collections);
  const selected = $derived.by(() => {
    const membership = collectionMembership(collections);
    return new Set(gameIds.flatMap((id) => [...(membership.get(id) ?? [])]));
  });

  async function run(task: () => Promise<void>): Promise<void> {
    pending = true;
    error = null;
    try {
      await task();
    } catch (cause) {
      error = toMessage(cause);
    } finally {
      pending = false;
    }
  }

  function toggle(id: string, member: boolean): Promise<void> {
    const next = [...selected].filter((item) => item !== id);
    if (member) next.push(id);
    return run(() => setCollections(gameIds, next));
  }

  function create(): Promise<void> {
    return run(async () => {
      const id = await addCollection(newName);
      newName = "";
      await setCollections(gameIds, [...selected, id]);
    });
  }
</script>

<Dialog open title={t("Organizza {0}", $language, [name])} dismissible={!pending} {onClose}>
  {#snippet children(dismiss)}
    <SelectField
      id="organize-status"
      label={t("Stato", $language)}
      value={playStatus}
      options={playStatusOptions($language)}
      disabled={pending}
      onChange={(value) => void run(() => setPlayStatus(gameIds, value as PlayStatus))}
    />
    <fieldset class="flex flex-col gap-2" disabled={pending}>
      <legend class="mb-2 text-sm font-semibold text-legio-fg">{t("Raccolte", $language)}</legend>
      {#each collections as collection (collection.id)}
        <Toggle label={collection.name} checked={selected.has(collection.id)} disabled={pending} onChange={(checked) => void toggle(collection.id, checked)} />
      {:else}
        <p class="text-sm text-legio-fg-muted">{t("Nessuna raccolta. Creane una qui sotto.", $language)}</p>
      {/each}
    </fieldset>
    <form class="flex items-end gap-2" onsubmit={(event) => { event.preventDefault(); if (newName.trim()) void create(); }}>
      <TextField id="organize-new-collection" class="min-w-0 flex-1" label={t("Nuova raccolta", $language)} bind:value={newName} maxlength={maxCollectionNameLength} disabled={pending} />
      <Button type="submit" label={t("Crea", $language)} variant="secondary" disabled={pending || newName.trim() === ""} />
    </form>
    {#if error !== null}<ErrorBanner message={error} />{/if}
    <div class="flex justify-end">
      <Button label={t("Fine", $language)} disabled={pending} onClick={dismiss} />
    </div>
  {/snippet}
</Dialog>
