<script lang="ts">
  import Sidebar from "./lib/components/layout/Sidebar.svelte";
  import MainContainer from "./lib/components/layout/MainContainer.svelte";
  import TitleBar from "./lib/components/layout/TitleBar.svelte";
</script>

<svelte:head>
  <title>Legio</title>
</svelte:head>

<div class="relative h-dvh bg-black">
  <div class="flex h-full min-h-0 gap-2 p-2">
    <Sidebar />
    <MainContainer />
  </div>
  <TitleBar />
</div>