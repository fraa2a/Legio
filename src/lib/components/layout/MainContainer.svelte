<script lang="ts">
  import DownloadsView from "../../features/downloads/DownloadsView.svelte";
  import HomeView from "../../features/home/HomeView.svelte";
  import LibraryView from "../../features/library/LibraryView.svelte";
  import SettingsView from "../../features/settings/SettingsView.svelte";
  import StoreView from "../../features/store/StoreView.svelte";
  import { activeSection } from "../../stores/navigation";
</script>

<main class="min-h-0 min-w-0 flex-1 overflow-y-auto rounded-2xl bg-zinc-900 p-8 light:bg-zinc-50">
  {#if $activeSection === "home"}
    <HomeView />
  {:else if $activeSection === "library"}
    <LibraryView />
  {:else if $activeSection === "store"}
    <StoreView />
  {:else if $activeSection === "downloads"}
    <DownloadsView />
  {:else}
    <SettingsView />
  {/if}
</main>
