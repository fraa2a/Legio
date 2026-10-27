<script lang="ts">
  let { value, active = true, compact = false }: { value: number; active?: boolean; compact?: boolean } = $props();
  const percent = $derived(Math.max(0, Math.min(100, Number.isFinite(value) ? value : 0)));

  // Wavelength and amplitude in CSS pixels, so the shape keeps its proportions at any container size.
  const length = $derived(compact ? 32 : 72);
  const amplitude = $derived(compact ? 2.5 : 4);
  const surface = $derived(wavePath(length, amplitude));
  // The front wave sits lower so the back one shows above it between their crests.
  const rise = $derived(amplitude * 0.8);

  // Crests sit one pixel below the top so the crest stroke is not clipped.
  // Draws enough periods to cover any sidebar width plus one period of drift.
  function wavePath(waveLength: number, waveAmplitude: number): string {
    const half = waveLength / 2;
    const middle = waveAmplitude + 1;
    const periods = Math.ceil(1024 / waveLength);
    let path = `M0 ${middle} Q${half / 2} ${1 - waveAmplitude} ${half} ${middle}`;
    for (let step = 2; step <= periods * 2; step++) path += ` T${step * half} ${middle}`;
    return path;
  }
</script>

<div
  class="wave-fill pointer-events-none absolute inset-0 overflow-hidden"
  style:--wave-length="{length}px"
  style:--wave-rise="{rise}px"
  style:--wave-depth="{amplitude * 2 + 2 + rise}px"
  aria-hidden="true"
>
  <div class="wave-level" style:transform="translateY({100 - percent}%)">
    <svg class="wave wave-back" class:moving={active}>
      <path d="{surface} V4096 H0 Z" />
    </svg>
    <svg class="wave wave-front" class:moving={active}>
      <path d="{surface} V4096 H0 Z" />
      <path class="wave-crest" d={surface} />
    </svg>
  </div>
</div>

<style>
  .wave-fill {
    --wave-back: var(--color-legio-download);
    --wave-front: color-mix(in oklab, var(--color-legio-download) 72%, black);
    --wave-crest: color-mix(in oklab, var(--color-legio-download) 55%, white);
  }

  :global([data-theme="light"]) .wave-fill {
    --wave-back: color-mix(in oklab, var(--color-legio-download) 22%, white);
    --wave-front: color-mix(in oklab, var(--color-legio-download) 40%, white);
    --wave-crest: color-mix(in oklab, var(--color-legio-download) 70%, white);
  }

  /* Spans the container plus both wave extremes and the crest stroke,
     so 0% hides every crest and 100% covers every trough. */
  .wave-level {
    position: absolute;
    inset-inline: 0;
    top: calc(var(--wave-depth) * -1);
    height: calc(100% + var(--wave-depth));
    transition: transform 750ms linear;
  }

  .wave {
    position: absolute;
    top: 0;
    left: 0;
    width: calc(100% + var(--wave-length));
    height: 100%;
    animation: wave-drift 2.6s linear infinite paused;
  }

  .wave-back {
    fill: var(--wave-back);
    animation-duration: 4.2s;
    animation-direction: reverse;
  }

  .wave-front {
    top: var(--wave-rise);
    fill: var(--wave-front);
  }

  .wave-crest {
    fill: none;
    stroke: var(--wave-crest);
    stroke-width: 1.5;
  }

  .wave.moving {
    animation-play-state: running;
  }

  @keyframes wave-drift {
    to { transform: translateX(calc(var(--wave-length) * -1)); }
  }

  :global(html[data-motion-paused="true"]) .wave {
    animation-play-state: paused;
  }

  @media (prefers-reduced-motion: reduce) {
    .wave { animation: none; }
    .wave-level { transition: none; }
  }
</style>
