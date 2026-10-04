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

  let rendered: string[] = $state(["home"]);
  let mounted = $state(false);

  const target = $derived($selectedGameId === null ? $activeSection : `${gamePrefix}${$selectedGameId}`);

  onMount(() => void (mounted = true));

  $effect(() => {
    void target;
    if (rendered[0] !== target) rendered = [target];
  });
</script>

<main
  class="relative min-h-0 min-w-0 flex-1 overflow-hidden rounded-xl scrollbar-none bg-black light:bg-zinc-50"
>
  {#each rendered as id (id)}
    <div
      data-page={id}
      class="absolute inset-0 overflow-y-auto scrollbar-none"
      in:blurFade={{ duration: mounted ? undefined : 0 }}
      out:blurFade
    >
      {#if id === "home"}
        <HomeView />
      {:else if id === "library"}
        <LibraryView />
      {:else if id.startsWith(gamePrefix)}
        <GameDetailView gameId={id.slice(gamePrefix.length)} />
      {:else if id === "store"}
        <StoreView />
      {:else}
        <DownloadsView />
      {/if}
    </div>
  {/each}
</main>
