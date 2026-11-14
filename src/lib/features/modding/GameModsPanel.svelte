<script lang="ts">
  import { onMount } from "svelte";
  import { t, language } from "../../i18n";
  import Button from "../../components/ui/Button.svelte";
  import Callout from "../../components/ui/Callout.svelte";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import Panel from "../../components/ui/Panel.svelte";
  import SelectField from "../../components/ui/SelectField.svelte";
  import { launchStateByGame } from "../../stores/launch";
  import {
    busyProfiles,
    installLoader,
    loadModdableGames,
    loadOnce,
    modLaunchStatus,
    moddableGameById,
    modProfiles,
    profileMods,
    selectProfile,
  } from "../../stores/modding";
  import { openModManager } from "../../stores/navigation";
  import { showToast } from "../../stores/toast";
  import { toMessage } from "../../utils/errors";
  import {
    activeProfile,
    classifyModdingError,
    describeModdingError,
    installNotice,
    launchNotice,
    moddableGameState,
    profileOptions,
    vanillaProfileValue,
  } from "./modding-model";

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
  const launchStatus = $derived(modLaunchStatus(gameId));
  const busy = $derived(profile !== null && $busyProfiles.has(profile.id));
  const loaderMissing = $derived(profile !== null && launchNotice($launchStatus.data) === "loader-missing");

  let error = $state<string | null>(null);
  let pending = $state(false);
  let revision = $state(0);

  $effect(() => {
    if (ready && bound) loadOnce(profiles);
  });

  $effect(() => {
    if (mods !== null) loadOnce(mods);
  });

  $effect(() => {
    if (bound && profile !== null) loadOnce(launchStatus);
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

  async function installBepInEx(profileId: string): Promise<void> {
    pending = true;
    error = null;
    try {
      const result = installNotice(await installLoader(gameId, profileId), $language);
      showToast(result.message, result.tone === "warning" ? "warning" : "success");
    } catch (cause) {
      const failure = classifyModdingError(cause, $language);
      if (failure.kind === "cancelled") showToast(failure.message, "info");
      else error = failure.message;
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
            disabled={running || pending || busy}
            onChange={(value) => void change(value)}
          />
        {/key}
        <p class="text-sm text-legio-fg-muted">
          {#if profile === null}
            {t("Il gioco si avvia senza mod.", $language)}
          {:else if modCount === 1}
            {t("1 mod in questo profilo.", $language)}
          {:else}
            {t("{0} mod in questo profilo.", $language, [modCount])}
          {/if}
        </p>
      {:else}
        <p class="text-sm text-legio-fg-muted">
          {t("Mod disponibili su Thunderstore per {0}. Apri il gestore per creare il primo profilo.", $language, [game.displayName ?? game.name])}
        </p>
      {/if}
      {#if loaderMissing && profile !== null}
        {@const target = profile}
        <Callout
          tone="warning"
          variant="inline"
          message={t("Questo profilo non ha BepInEx: il gioco non si avvierà finché non lo installi o scegli Vanilla.", $language)}
          onRetry={running || busy || pending ? undefined : () => void installBepInEx(target.id)}
          retryLabel={t("Installa BepInEx", $language)}
        />
      {/if}
      {#if running && bound}<p class="text-xs text-legio-fg-subtle">{t("Chiudi il gioco per cambiare profilo.", $language)}</p>{/if}
      {#if error !== null}<ErrorBanner message={error} />{/if}
    </div>
  </Panel>
{/if}
