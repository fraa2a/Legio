<script lang="ts">
  import { flushSync, onDestroy, onMount, untrack } from "svelte";
  import ArtworkScope from "../ui/ArtworkScope.svelte";
  import DownloadsView from "../../features/downloads/DownloadsView.svelte";
  import HomeView from "../../features/home/HomeView.svelte";
  import GameDetailView from "../../features/library/GameDetailView.svelte";
  import LibraryView from "../../features/library/LibraryView.svelte";
  import StoreView from "../../features/store/StoreView.svelte";
  import { activeSection, selectedGameId } from "../../stores/navigation";
  import { pageTransitionDuration, reducedMotion } from "../../utils/motion";
  import { logApplicationEvent } from "../../services/application-log";

  import { animateForeground, type ForegroundAnimation } from "../../utils/foreground-transition";

  const gamePrefix = "library:";

  let mounted = $state(false);

  const target = $derived($selectedGameId === null ? $activeSection : `${gamePrefix}${$selectedGameId}`);

  let displayedTarget = $state(untrack(() => target));
  let page: HTMLDivElement | undefined = $state();
  let transition: ForegroundAnimation | undefined;
  let navigationRequest = 0;

  onMount(() => void (mounted = true));
  onDestroy(() => {
    mounted = false;
    navigationRequest++;
    transition?.cancel();
  });

  function pageName(value: string): string {
    return value.startsWith(gamePrefix) ? "game_details" : value;
  }

  $effect(() => {
    logApplicationEvent("info", "navigation", `Displayed: ${pageName(displayedTarget)}`);
  });

  async function transitionTo(next: string, request: number): Promise<void> {
    const duration = reducedMotion ? 0 : pageTransitionDuration / 2;
    const previous = page;
    const outgoing = previous ? animateForeground(previous, "out", duration) : undefined;
    transition = outgoing;
    await outgoing?.finished;
    if (!mounted || target !== next || navigationRequest !== request) return;
    logApplicationEvent("info", "navigation", `DOM update starting: ${pageName(next)}`);
    flushSync(() => { displayedTarget = next; });
    outgoing?.cancel();
    logApplicationEvent("info", "navigation", `DOM update completed: ${pageName(next)}`);
    const incoming = page ? animateForeground(page, "in", duration) : undefined;
    transition = incoming;
    await incoming?.finished;
    if (!mounted || target !== next || navigationRequest !== request) return;
    incoming?.cancel();
    if (transition === incoming) transition = undefined;
    logApplicationEvent("info", "navigation", `Transition finished: ${pageName(next)}`);
  }

  $effect(() => {
    const next = target;
    const request = ++navigationRequest;
    transition?.cancel();
    if (next === untrack(() => displayedTarget)) return;
    logApplicationEvent("info", "navigation", `Requested: ${pageName(next)}; transition: foreground`);
    if (!mounted || reducedMotion) {
      displayedTarget = next;
      return;
    }
    untrack(() => { void transitionTo(next, request); });
  });


</script>

<main
  class="legio-content relative min-h-0 min-w-0 flex-1 overflow-hidden rounded-xl scrollbar-none"
>
  {#key displayedTarget}
    <ArtworkScope active={displayedTarget === target}>
      <div
        bind:this={page}
        data-page={displayedTarget}
        class="absolute inset-0 overflow-y-auto scrollbar-none"
      >
        {#if displayedTarget === "home"}
          <HomeView />
        {:else if displayedTarget === "library"}
          <LibraryView />
        {:else if displayedTarget.startsWith(gamePrefix)}
          <GameDetailView gameId={displayedTarget.slice(gamePrefix.length)} />
        {:else if displayedTarget === "store"}
          <StoreView />
        {:else}
          <DownloadsView />
        {/if}
      </div>
    </ArtworkScope>
  {/key}
</main>
