<script lang="ts">
  import type { Snippet } from "svelte";
  import { artworkDemand, provideArtworkDemand } from "../../utils/artwork-demand";
  let { active, children }: { active: boolean; children: Snippet } = $props();
  const parent = artworkDemand();
  provideArtworkDemand(() => parent() && active);
</script>

{@render children()}
