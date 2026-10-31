<script lang="ts">
  import { onMount } from "svelte";
  import { t, language } from "../../i18n";
  import Button from "../../components/ui/Button.svelte";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import Panel from "../../components/ui/Panel.svelte";
  import SelectField from "../../components/ui/SelectField.svelte";
  import { launchStateByGame } from "../../stores/launch";
  import { loadModdableGames, loadOnce, moddableGameById, modProfiles, profileMods, selectProfile } from "../../stores/modding";
  import { openModManager } from "../../stores/navigation";
  import { toMessage } from "../../utils/errors";
  import { activeProfile, describeModdingError, moddableGameState, profileOptions, vanillaProfileValue } from "./modding-model";

  let { gameId }: { gameId: string } = $props();

  onMount(() => void loadModdableGames());

  const game = $derived($moddableGameById.get(gameId) ?? null);
  const ready = $derived(game !== null && moddableGameState(game) === "ready");
  const bound = $derived(game?.binding != null);
  const profiles = $derived(modProfiles(gameId));
  const profile = $derived(activeProfile(game, $profiles.data));
  const mods = $derived(profile === null ? null : profileMods(profile.id));
  const modCount = $derived($mods?.data.length ?? 0);
  const running = $derived(($launchStateByGame.get(gameId)?.status ?? "idle") !== "idle");

  let error = $state<string | null>(null);
  let pending = $state(false);
  let revision = $state(0);

  $effect(() => {
    if (ready && bound) loadOnce(profiles);
  });

  $effect(() => {
    if (mods !== null) loadOnce(mods);
  });

  async function change(value: string): Promise<void> {
    pending = true;
    error = null;
    try {
      await selectProfile(gameId, value === vanillaProfileValue ? null : value);
    } catch (cause) {
      error = describeModdingError(toMessage(cause), $language);
      revision++;
    } finally {
      pending = false;
    }
  }
</script>

{#if game !== null && ready}
  <Panel title={t("Mod", $language)}>
    {#snippet actions()}
      <Button label={t("Gestisci mod", $language)} variant="secondary" size="sm" onClick={() => openModManager(gameId, "game")} />
    {/snippet}
    <div class="flex flex-col gap-3">
      {#if bound}
        {#key revision}
          <SelectField
            id="detail-mod-profile"
            label={t("Profilo attivo", $language)}
            value={profile?.id ?? vanillaProfileValue}
            options={profileOptions($profiles.data, $language)}
            disabled={running || pending}
            onChange={(value) => void change(value)}
          />
        {/key}
        <p class="text-sm text-zinc-400 light:text-zinc-600">
          {#if profile === null}
            {t("Il gioco si avvia senza mod.", $language)}
          {:else if modCount === 1}
            {t("1 mod in questo profilo.", $language)}
          {:else}
            {t("{0} mod in questo profilo.", $language, [modCount])}
          {/if}
        </p>
      {:else}
        <p class="text-sm text-zinc-400 light:text-zinc-600">
          {t("Mod disponibili su Thunderstore per {0}. Apri il gestore per creare il primo profilo.", $language, [game.displayName ?? game.name])}
        </p>
      {/if}
      {#if running && bound}<p class="text-xs text-zinc-500">{t("Chiudi il gioco per cambiare profilo.", $language)}</p>{/if}
      {#if error !== null}<ErrorBanner message={error} />{/if}
    </div>
  </Panel>
{/if}
