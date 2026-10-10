<script lang="ts">
  import { flushSync, onDestroy, onMount, untrack } from "svelte";
  import DownloadsView from "../../features/downloads/DownloadsView.svelte";
  import HomeView from "../../features/home/HomeView.svelte";
  import GameDetailView from "../../features/library/GameDetailView.svelte";
  import LibraryView from "../../features/library/LibraryView.svelte";
  import StoreView from "../../features/store/StoreView.svelte";
  import { activeSection, selectedGameId } from "../../stores/navigation";
  import { pageTransition, reducedMotion } from "../../utils/motion";
  import { logApplicationEvent, logNavigationCheckpoint } from "../../services/application-log";

  const gamePrefix = "library:";

  let mounted = $state(false);

  const target = $derived($selectedGameId === null ? $activeSection : `${gamePrefix}${$selectedGameId}`);

  let displayedTarget = $state(untrack(() => target));
  let transition: ViewTransition | undefined;
  let navigationRequest = 0;
  const nativeTransitions = typeof document.startViewTransition === "function";

  onMount(() => void (mounted = true));
  onDestroy(() => {
    mounted = false;
    navigationRequest++;
    transition?.skipTransition();
  });

  function pageName(value: string): string {
    return value.startsWith(gamePrefix) ? "game_details" : value;
  }

  $effect(() => {
    logApplicationEvent("info", "navigation", `Displayed: ${pageName(displayedTarget)}`);
  });

  function startNativeTransition(next: string, request: number): void {
    if (!mounted || target !== next || navigationRequest !== request) return;
    const current = document.startViewTransition(async () => {
      const before = logNavigationCheckpoint(`DOM update starting: ${pageName(next)}`);
      if (before !== undefined) await before;
      if (!mounted || target !== next || navigationRequest !== request) return;
      flushSync(() => {
        displayedTarget = next;
      });
      const after = logNavigationCheckpoint(`DOM update completed: ${pageName(next)}`);
      if (after !== undefined) await after;
    });
    transition = current;
    void current.ready.then(() => {
      logApplicationEvent("info", "navigation", `Snapshot ready: ${pageName(next)}`);
    }).catch((error: unknown) => {
      if (!(error instanceof DOMException && error.name === "AbortError")) {
        console.warn("Page transition snapshot failed", error);
      }
    });
    void current.finished.then(() => {
      logApplicationEvent("info", "navigation", `Transition finished: ${pageName(next)}`);
      if (transition === current) transition = undefined;
    }).catch((error: unknown) => console.warn("Page transition failed", error));
  }

  $effect(() => {
    const next = target;
    const request = ++navigationRequest;
    transition?.skipTransition();
    if (next === untrack(() => displayedTarget)) return;
    logApplicationEvent("info", "navigation", `Requested: ${pageName(next)}; transition: ${nativeTransitions && !reducedMotion ? "native" : "fade"}`);
    if (!nativeTransitions || reducedMotion || !mounted) {
      displayedTarget = next;
      return;
    }
    const checkpoint = logNavigationCheckpoint(`Snapshot starting: ${pageName(next)}`);
    if (checkpoint === undefined) startNativeTransition(next, request);
    else void checkpoint.then(() => startNativeTransition(next, request));
  });


</script>

<main
  class="legio-content relative min-h-0 min-w-0 flex-1 overflow-hidden rounded-xl scrollbar-none"
>
  {#key displayedTarget}
    <div
      data-page={displayedTarget}
      class="absolute inset-0 overflow-y-auto scrollbar-none"
      in:pageTransition={{ duration: mounted && !nativeTransitions ? undefined : 0 }}
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
  {/key}
</main>
