<script lang="ts">
  let { value, active = true }: { value: number; active?: boolean } = $props();
  const percent = $derived(Math.max(0, Math.min(100, Number.isFinite(value) ? value : 0)));
</script>

{#if percent > 0}
  <div class="pointer-events-none absolute inset-0 overflow-hidden" aria-hidden="true">
    <div class="wave-fill absolute inset-x-0 bottom-0 bg-legio-download text-legio-download" style:height={`${percent}%`}>
      <svg class="wave-surface" class:moving={active} viewBox="0 0 240 12" preserveAspectRatio="none">
        <path d="M0 6 C20 -2 40 -2 60 6 S100 14 120 6 S160 -2 180 6 S220 14 240 6 V12 H0 Z" fill="currentColor" />
      </svg>
    </div>
  </div>
{/if}

<style>
  .wave-fill { opacity: 0.4; transition: height 220ms ease-out; }
  .wave-surface { position: absolute; top: -6px; left: 0; width: 200%; height: 12px; animation: wave-drift 3s linear infinite; animation-play-state: paused; }
  .wave-surface.moving { animation-play-state: running; }
  @keyframes wave-drift { from { transform: translateX(0); } to { transform: translateX(-50%); } }
  :global(html[data-motion-paused="true"]) .wave-surface { animation-play-state: paused; }
  @media (prefers-reduced-motion: reduce) { .wave-surface { animation: none; } .wave-fill { transition: none; } }
</style>
