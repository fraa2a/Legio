<script lang="ts">
  let { value, active = true, compact = false }: { value: number; active?: boolean; compact?: boolean } = $props();
  const percent = $derived(Math.max(0, Math.min(100, Number.isFinite(value) ? value : 0)));
</script>

{#if percent > 0}
  <div class="pointer-events-none absolute inset-0 overflow-hidden" aria-hidden="true">
    <div class="wave-fill absolute inset-x-0 bottom-0 text-legio-download" class:compact style:height={`${percent}%`}>
      <svg class="wave-surface" class:moving={active} viewBox="0 0 240 12" preserveAspectRatio="none">
        <path d="M0 6 C10 2.7 20 0 30 0 C40 0 50 2.7 60 6 C70 9.3 80 12 90 12 C100 12 110 9.3 120 6 C130 2.7 140 0 150 0 C160 0 170 2.7 180 6 C190 9.3 200 12 210 12 C220 12 230 9.3 240 6 V12 H0 Z" fill="currentColor" />
      </svg>
    </div>
  </div>
{/if}

<style>
  .wave-fill { --wave-height: 12px; background: linear-gradient(to bottom, transparent calc(var(--wave-height) / 2), currentColor 0); opacity: 0.4; transition: height 220ms ease-out; }
  .wave-fill.compact { --wave-height: 6px; }
  .wave-surface { position: absolute; top: calc(var(--wave-height) / -2); left: 0; width: 200%; height: var(--wave-height); animation: wave-drift 3s linear infinite; animation-play-state: paused; }
  .wave-surface.moving { animation-play-state: running; }
  @keyframes wave-drift { from { left: 0; } to { left: -100%; } }
  :global(html[data-motion-paused="true"]) .wave-surface { animation-play-state: paused; }
  @media (prefers-reduced-motion: reduce) { .wave-surface { animation: none; } .wave-fill { transition: none; } }
</style>
