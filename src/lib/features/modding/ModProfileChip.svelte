<script lang="ts">
  import { t, language } from "../../i18n";
  import Icon from "../../components/ui/Icon.svelte";
  import { loadModdableGames, loadOnce, moddableGameById, modProfiles } from "../../stores/modding";
  import { openModManager } from "../../stores/navigation";
  import { activeProfile } from "./modding-model";

  let { gameId }: { gameId: string } = $props();

  const game = $derived($moddableGameById.get(gameId) ?? null);
  const profiles = $derived(modProfiles(gameId));
  const profile = $derived(activeProfile(game, $profiles.data));

  $effect(() => {
    void loadModdableGames();
  });

  $effect(() => {
    if (game?.binding?.activeProfileId != null) loadOnce(profiles);
  });
</script>

{#if profile !== null}
  <button
    type="button"
    onclick={() => openModManager(gameId, "game")}
    aria-label={t("Profilo mod attivo: {0}. Gestisci mod", $language, [profile.name])}
    class="flex h-15 max-w-64 min-w-0 items-center gap-3 rounded-xl border border-white/10 legio-glass px-3 text-left text-zinc-100 transition-colors hover:bg-white/10 light:border-zinc-900/10 light:text-zinc-900 light:hover:bg-zinc-900/5"
  >
    <span class="flex size-10 shrink-0 items-center justify-center rounded-md bg-white/10 text-zinc-200 light:bg-zinc-900/10 light:text-zinc-700">
      <Icon name="mods" />
    </span>
    <span class="flex min-w-0 flex-col gap-0.5">
      <span class="truncate text-xs leading-4 text-zinc-400 light:text-zinc-600">{t("Profilo mod", $language)}</span>
      <span class="truncate text-base leading-5 font-semibold">{profile.name}</span>
    </span>
  </button>
{/if}
