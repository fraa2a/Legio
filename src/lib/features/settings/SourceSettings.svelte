<script lang="ts">
  import { t, language } from "../../i18n";
  import SettingsGroup from "../../components/ui/SettingsGroup.svelte";
  import TextField from "../../components/ui/TextField.svelte";
  import Button from "../../components/ui/Button.svelte";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import { source, sourceBusy, sourceActionError, sourceRefreshError, addSource, removeSource, refreshSource } from "../../stores/source";
  import { sourceLinkRegistrationError } from "../../stores/source-links";

  let url = $state("");

  async function submit(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    if (await addSource(url)) url = "";
  }
</script>

<div class="flex flex-col gap-5">
  <SettingsGroup title={t("Sorgenti dei giochi", $language)} icon="globe">
    <p class="text-sm text-zinc-400 light:text-zinc-600">{t("Aggiungi un catalogo HTTPS per rendere disponibili i suoi giochi nello Store.", $language)}</p>
    <form class="flex flex-col gap-3" onsubmit={submit}>
      <TextField id="source-url" label={t("URL della sorgente", $language)} bind:value={url} placeholder="https://catalogo.example/games.json" disabled={$sourceBusy} />
      <div class="flex justify-end">
        <Button type="submit" label={t("Aggiungi sorgente", $language)} disabled={$sourceBusy || !url.trim()} />
      </div>
    </form>
    {#if $sourceActionError}<ErrorBanner message={$sourceActionError} />{/if}
    {#if $source.error}<ErrorBanner message={$source.error} onRetry={() => { void source.load(); }} />{/if}
  </SettingsGroup>

  <SettingsGroup title={t("Sorgenti installate", $language)} icon="library">
    {#if $source.data.sources.length === 0}
      <p class="text-sm text-zinc-400 light:text-zinc-600">{t("Nessuna sorgente installata.", $language)}</p>
    {:else}
      <ul class="flex flex-col gap-3">
        {#each $source.data.sources as installed (installed.id)}
          <li class="flex flex-col gap-3 rounded-lg bg-white/5 p-3 light:bg-zinc-900/5">
            <p class="break-all text-sm text-zinc-100 light:text-zinc-900">{installed.url}</p>
            <div class="flex flex-wrap items-center justify-between gap-3">
              <p class="text-xs text-zinc-400 light:text-zinc-600">{installed.gameCount} {t("giochi", $language)} · {t(installed.stale ? "Cache da aggiornare" : "Aggiornata", $language)}</p>
              <Button label={t("Rimuovi", $language)} variant="secondary" disabled={$sourceBusy} onClick={() => { void removeSource(installed.id); }} />
            </div>
            {#if installed.warning}<ErrorBanner message={installed.warning} />{/if}
          </li>
        {/each}
      </ul>
      <div class="flex justify-end">
        <Button label={t("Aggiorna sorgenti", $language)} variant="secondary" disabled={$sourceBusy} onClick={() => { void refreshSource(); }} />
      </div>
    {/if}
    {#if $sourceRefreshError}<ErrorBanner message={$sourceRefreshError} />{/if}
    {#if $source.data.warning}<ErrorBanner message={$source.data.warning} />{/if}
  </SettingsGroup>
  <p class="text-xs text-zinc-500">{t("Puoi aggiungere una sorgente anche aprendo un collegamento legio://add-source?url=... dopo il primo avvio di Legio.", $language)}</p>
  {#if $sourceLinkRegistrationError}<ErrorBanner message={$sourceLinkRegistrationError} />{/if}
</div>
