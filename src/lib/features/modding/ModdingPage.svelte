<script lang="ts">
  import { onMount } from "svelte";
  import { t, language } from "../../i18n";
  import Button from "../../components/ui/Button.svelte";
  import EmptyState from "../../components/ui/EmptyState.svelte";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import StateBlock from "../../components/ui/StateBlock.svelte";
  import type { ModdableGame } from "../../services/modding";
  import { bindGame, moddableGames } from "../../stores/modding";
  import { openModManager, selectSection } from "../../stores/navigation";
  import EcosystemGameDialog from "./EcosystemGameDialog.svelte";
  import ModCachePanel from "./ModCachePanel.svelte";
  import ModdableGameCard from "./ModdableGameCard.svelte";
  import { moddableGameState, sortModdableGames } from "./modding-model";

  // The library may have changed since the last visit.
  onMount(() => void moddableGames.load());

  let bindingTarget = $state<ModdableGame | null>(null);

  const sorted = $derived(sortModdableGames($moddableGames.data));
  const ready = $derived(sorted.filter((game) => moddableGameState(game) === "ready"));
  const others = $derived(sorted.filter((game) => moddableGameState(game) !== "ready"));

  function open(game: ModdableGame): void {
    if (moddableGameState(game) === "ready") openModManager(game.gameId);
    else bindingTarget = game;
  }

  async function bind(ecosystemGameId: string): Promise<void> {
    const game = bindingTarget;
    if (game === null) return;
    await bindGame(game.gameId, ecosystemGameId);
  }
</script>

<div class="flex min-h-full flex-col gap-4">
  {#if $moddableGames.error !== null && $moddableGames.data.length > 0}
    <ErrorBanner message={$moddableGames.error} onRetry={() => void moddableGames.reload()} retryLabel={t("Riprova", $language)} />
  {/if}

  <StateBlock
    status={$moddableGames.status}
    hasData={$moddableGames.data.length > 0}
    loadingMessage={t("Caricamento dei giochi...", $language)}
    skeletonRows={2}
    skeletonClass="h-36"
    error={$moddableGames.error}
    onRetry={() => void moddableGames.reload()}
  />

  {#if $moddableGames.status === "empty"}
    <EmptyState
      icon="mods"
      title={t("Nessun gioco in libreria", $language)}
      description={t("Aggiungi un gioco alla libreria per installare le sue mod da Thunderstore.", $language)}
    >
      <Button label={t("Vai alla libreria", $language)} variant="primary" onClick={() => selectSection("library")} />
    </EmptyState>
  {/if}

  {#if ready.length > 0}
    <section class="flex flex-col gap-3" aria-labelledby="mods-ready-title">
      <div>
        <h2 id="mods-ready-title" class="text-base font-semibold text-zinc-100 light:text-zinc-900">{t("Giochi con mod", $language)}</h2>
        <p class="mt-0.5 text-sm text-zinc-400 light:text-zinc-600">{t("Le mod vivono nei profili di Legio: la cartella del gioco resta pulita e puoi tornare alla versione vanilla in ogni momento.", $language)}</p>
      </div>
      <ul class="grid auto-rows-max grid-cols-[repeat(auto-fill,minmax(min(100%,18rem),1fr))] gap-4">
        {#each ready as game (game.gameId)}
          <ModdableGameCard {game} onOpen={() => open(game)} />
        {/each}
      </ul>
    </section>
  {/if}

  {#if others.length > 0}
    <section class="flex flex-col gap-3" aria-labelledby="mods-other-title">
      <div>
        <h2 id="mods-other-title" class="text-base font-semibold text-zinc-100 light:text-zinc-900">{t("Altri giochi", $language)}</h2>
        <p class="mt-0.5 text-sm text-zinc-400 light:text-zinc-600">{t("Per ora Legio installa le mod dei giochi che usano BepInEx. Collega a mano i giochi che Thunderstore non ha riconosciuto.", $language)}</p>
      </div>
      <ul class="grid auto-rows-max grid-cols-[repeat(auto-fill,minmax(min(100%,18rem),1fr))] gap-4">
        {#each others as game (game.gameId)}
          <ModdableGameCard {game} onOpen={() => open(game)} />
        {/each}
      </ul>
    </section>
  {/if}

  <ModCachePanel />
</div>

<EcosystemGameDialog
  open={bindingTarget !== null}
  gameName={bindingTarget?.name ?? ""}
  currentId={bindingTarget?.binding?.ecosystemGameId ?? null}
  suggestedId={bindingTarget?.ecosystemGameId ?? null}
  onClose={() => (bindingTarget = null)}
  onSelect={bind}
/>
