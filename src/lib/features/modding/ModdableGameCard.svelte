<script lang="ts">
  import { t, language } from "../../i18n";
  import ArtworkTile from "../../components/ui/ArtworkTile.svelte";
  import Badge from "../../components/ui/Badge.svelte";
  import Icon from "../../components/ui/Icon.svelte";
  import type { ModdableGame } from "../../services/modding";
  import { games } from "../../stores/games";
  import { loadOnce, moddableGames, modProfiles, profileMods } from "../../stores/modding";
  import { hasDuplicateName, instanceLocation } from "../library/instance-options";
  import { activeProfile, canChooseEcosystemGame, moddableGameState } from "./modding-model";

  let { game, onOpen }: { game: ModdableGame; onOpen: () => void } = $props();

  const modState = $derived(moddableGameState(game));
  const interactive = $derived(modState === "ready" || canChooseEcosystemGame(game));
  const bound = $derived(game.binding !== null);
  const profiles = $derived(modProfiles(game.gameId));
  const profile = $derived(activeProfile(game, $profiles.data));
  const mods = $derived(profile === null ? null : profileMods(profile.id));
  const modCount = $derived(mods === null ? 0 : $mods?.data.length ?? 0);
  const location = $derived(hasDuplicateName(game, $moddableGames.data) ? instanceLocation($games.data.find((entry) => entry.id === game.gameId)) : null);
  const monogram = $derived(game.name.trim().charAt(0).toUpperCase() || "?");

  $effect(() => {
    if (bound) loadOnce(profiles);
  });

  $effect(() => {
    if (mods !== null) loadOnce(mods);
  });

  const summary = $derived.by(() => {
    if (modState === "unmatched") return t("Nessuna corrispondenza su Thunderstore", $language);
    if (modState === "unsupported") return t("Il loader di {0} non è ancora supportato", $language, [game.displayName ?? game.name]);
    if (!bound) return t("Pronto per le mod", $language);
    if (profile === null) {
      const count = $profiles.data.length;
      if (count === 0) return t("Vanilla · nessun profilo", $language);
      return count === 1 ? t("Vanilla · 1 profilo", $language) : t("Vanilla · {0} profili", $language, [count]);
    }
    return modCount === 1 ? t("{0} · 1 mod", $language, [profile.name]) : t("{0} · {1} mod", $language, [profile.name, modCount]);
  });
</script>

<ArtworkTile steamAppId={game.steamAppId} {monogram} asset="header">
  {#if modState !== "ready"}
    <div class="absolute inset-0 bg-zinc-500/45 light:bg-zinc-300/60" aria-hidden="true"></div>
  {/if}
  <button
    type="button"
    class="relative flex flex-1 flex-col text-left focus-visible:-outline-offset-2 disabled:cursor-default"
    disabled={!interactive}
    aria-label={modState === "ready" ? t("Gestisci le mod di {0}", $language, [game.name]) : t("Collega {0} a Thunderstore", $language, [game.name])}
    onclick={onOpen}
  >
    <div class="absolute inset-x-2 top-2 flex items-center justify-end gap-1.5">
      {#if modState === "unsupported"}
        <Badge tone="warning" title={t("Loader non supportato", $language)} />
      {:else if modState === "unmatched"}
        <Badge title={t("Da collegare", $language)} />
      {:else if profile !== null}
        <span class="flex items-center gap-1 rounded-full bg-zinc-950/60 px-2 py-0.5 text-xs font-medium tabular-nums text-zinc-100 light:bg-zinc-100/70 light:text-zinc-900">
          <Icon name="mods" size="h-3.5 w-3.5" />
          {modCount}
        </span>
      {/if}
    </div>
    <div class="mt-auto bg-gradient-to-t from-zinc-950/85 via-zinc-950/55 to-transparent px-3 pt-10 pb-3 light:from-zinc-50/95 light:via-zinc-50/70">
      <p class="truncate font-semibold text-zinc-50 light:text-zinc-900">{game.name}</p>
      {#if location !== null}<p class="truncate text-xs text-zinc-400 light:text-zinc-600" title={location}>{location}</p>{/if}
      <p class="truncate text-xs text-zinc-300 light:text-zinc-700">{summary}</p>
    </div>
  </button>
</ArtworkTile>
