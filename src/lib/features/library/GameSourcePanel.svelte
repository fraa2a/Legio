<script lang="ts">
  import { t, language } from "../../i18n";
  import { source } from "../../stores/source";
  import { gameUpdates } from "../../stores/game-updates";
  import { bindGameSource, getGameSourceBinding } from "../../services/game-updates";
  import type { Game } from "../../services/local-state";
  import { toMessage } from "../../utils/errors";
  import Panel from "../../components/ui/Panel.svelte";
  import SelectField from "../../components/ui/SelectField.svelte";
  import TextField from "../../components/ui/TextField.svelte";
  import Toggle from "../../components/ui/Toggle.svelte";
  import Button from "../../components/ui/Button.svelte";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";

  let { game }: { game: Game } = $props();
  let selected = $state("");
  let version = $state("");
  let accepted = $state(false);
  let pending = $state(false);
  let error = $state<string | null>(null);
  let loadedGame: string | null = null;
  let request = 0;
  const variants = $derived(($source.data.manifest === null ? [] : [
    ...$source.data.manifest.verified.map(entry => ({ entry, verified: true })),
    ...$source.data.manifest.unverified.map(entry => ({ entry, verified: false })),
  ]).filter(({ entry }) => entry.steamAppId === game.steamAppId && entry.id !== null && entry.id !== undefined &&
    $source.data.sources.some(origin => origin.id === entry.sourceId && origin.supportGameUpdates)));
  const options = $derived([{ value: "", label: t("Seleziona una variante", $language) }, ...variants.map(({ entry }) => ({
    value: JSON.stringify([entry.sourceId, entry.id]), label: `${entry.name} (${entry.release.version}) - ${$source.data.sources.find(origin => origin.id === entry.sourceId)?.name ?? ""}`,
  }))]);
  const choice = $derived(variants.find(({ entry }) => JSON.stringify([entry.sourceId, entry.id]) === selected));
  $effect(() => {
    const id = game.id;
    if (game.steamInstallPath !== null || game.installationRoot === null || variants.length === 0 || loadedGame === id) return;
    loadedGame = id;
    const current = ++request;
    void getGameSourceBinding(id).then(binding => {
      if (current !== request) return;
      selected = binding.entryId === null ? "" : JSON.stringify([binding.sourceId, binding.entryId]);
      version = binding.installedVersion;
    }, reason => { if (current === request) error = toMessage(reason); });
  });
  async function save(): Promise<void> {
    if (choice?.entry.id === undefined || choice.entry.id === null || choice.entry.sourceId === undefined || choice.entry.sourceId === null) return;
    pending = true; error = null;
    try { await bindGameSource(game.id, choice.entry.sourceId, choice.entry.id, version, accepted); await gameUpdates.reload(); }
    catch (reason) { error = toMessage(reason); }
    finally { pending = false; }
  }
</script>

{#if game.steamInstallPath === null && game.installationRoot !== null && variants.length > 0}
  <Panel title={t("Aggiornamenti del gioco", $language)}>
    <SelectField id="game-update-source" label={t("Variante della source", $language)} value={selected} {options} disabled={pending} onChange={value => { selected = value; accepted = false; }} />
    <TextField id="game-installed-version" label={t("Versione installata", $language)} value={version} disabled={pending} oninput={value => (version = value)} />
    {#if choice !== undefined && !choice.verified}<Toggle label={t("Confermo questa source non verificata per gli aggiornamenti", $language)} checked={accepted} disabled={pending} onChange={value => (accepted = value)} />{/if}
    <div><Button label={t("Associa", $language)} disabled={pending || choice === undefined || !version.trim() || (!choice.verified && !accepted)} onClick={() => void save()} /></div>
    {#if error !== null}<ErrorBanner message={error} />{/if}
  </Panel>
{/if}
