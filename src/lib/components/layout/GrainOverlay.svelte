<script lang="ts">
  import { grainTexture, grainTileSize } from "./grain";

  let { intensity }: { intensity: number } = $props();

  const texture = grainTexture();
  // One noise texel per device pixel, as sharp as the screen allows.
  const tile = grainTileSize / Math.min(window.devicePixelRatio || 1, 2);
</script>

{#if texture !== null}
  <div class="legio-grain" style:opacity={intensity / 100} style:background-image={`url("${texture}")`} style:background-size={`${tile}px ${tile}px`} aria-hidden="true"></div>
{/if}

<style>
  .legio-grain {
    position: absolute;
    inset: 0;
    pointer-events: none;
    image-rendering: pixelated;
  }
</style>
