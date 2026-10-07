<script lang="ts">
  import { onMount } from "svelte";
  import DownloadsView from "../../features/downloads/DownloadsView.svelte";
  import HomeView from "../../features/home/HomeView.svelte";
  import GameDetailView from "../../features/library/GameDetailView.svelte";
  import LibraryView from "../../features/library/LibraryView.svelte";
  import StoreView from "../../features/store/StoreView.svelte";
  import { activeSection, selectedGameId } from "../../stores/navigation";
  import { blurFade } from "../../utils/motion";

  const gamePrefix = "library:";

  let mounted = $state(false);

  const target = $derived($selectedGameId === null ? $activeSection : `${gamePrefix}${$selectedGameId}`);

  onMount(() => void (mounted = true));


</script>

<main
  class="legio-content relative min-h-0 min-w-0 flex-1 overflow-hidden rounded-xl scrollbar-none"
>
  {#key target}
    <div
      data-page={target}
      class="absolute inset-0 overflow-y-auto scrollbar-none"
      in:blurFade={{ duration: mounted ? undefined : 0 }}
      out:blurFade
    >
      {#if target === "home"}
        <HomeView />
      {:else if target === "library"}
        <LibraryView />
      {:else if target.startsWith(gamePrefix)}
        <GameDetailView gameId={target.slice(gamePrefix.length)} />
      {:else if target === "store"}
        <StoreView />
      {:else}
        <DownloadsView />
      {/if}
    </div>
  {/key}
</main>
