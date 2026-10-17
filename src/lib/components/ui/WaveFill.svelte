<script lang="ts">
  let { value, active = true, compact = false }: { value: number; active?: boolean; compact?: boolean } = $props();
  const percent = $derived(Math.max(0, Math.min(100, Number.isFinite(value) ? value : 0)));
</script>

<div class="wave-fill pointer-events-none absolute inset-0 overflow-hidden text-legio-download" class:compact aria-hidden="true">
  <div class="wave-level" style:transform={`translateY(${100 - percent}%)`}>
    <div class="wave-surface" class:moving={active}></div>
  </div>
</div>

<style>
  .wave-fill {
    --wave-height: 12px;
    --wave-width: 96px;
    opacity: 0.7;
  }

  .wave-fill.compact {
    --wave-height: 6px;
    --wave-width: 48px;
  }

  .wave-level {
    position: absolute;
    inset-inline: 0;
    bottom: 0;
    height: calc(100% + var(--wave-height));
    background: linear-gradient(to bottom, transparent calc(var(--wave-height) - 0.5px), currentColor 0);
    transition: transform 750ms linear;
  }

  .wave-surface {
    position: absolute;
    inset: 0 0 auto;
    height: var(--wave-height);
    background-color: currentColor;
    mask-image: url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 120 12' preserveAspectRatio='none'%3E%3Cpath d='M0 6C10 2.7 20 0 30 0S50 2.7 60 6S80 12 90 12S110 9.3 120 6V12H0Z'/%3E%3C/svg%3E");
    mask-size: var(--wave-width) var(--wave-height);
    mask-repeat: repeat-x;
    animation: wave-drift 3s linear infinite paused;
  }

  .wave-surface.moving {
    animation-play-state: running;
  }

  @keyframes wave-drift {
    from { mask-position: 0 0; }
    to { mask-position: var(--wave-width) 0; }
  }

  :global(html[data-motion-paused="true"]) .wave-surface {
    animation-play-state: paused;
  }

  @media (prefers-reduced-motion: reduce) {
    .wave-surface { animation: none; }
    .wave-level { transition: none; }
  }
</style>
