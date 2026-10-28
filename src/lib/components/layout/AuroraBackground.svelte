<script lang="ts">
  let { opacity, color, active = true }: {
    opacity: number;
    color: string;
    active?: boolean;
  } = $props();
</script>

<div class="animated-background" class:paused={!active} style:opacity={opacity / 100} style:--aurora-color={color} aria-hidden="true">
  <span class="blob first"></span>
  <span class="blob second"></span>
  <span class="blob third"></span>
  <span class="grain"></span>
</div>

<style>
  .blob {
    position: absolute;
    width: 62vmax;
    height: 62vmax;
    border-radius: 50%;
    background: radial-gradient(closest-side, var(--blob-color), transparent);
    will-change: transform;
    animation: aurora-drift 42s ease-in-out infinite alternate;
  }
  .first {
    --blob-color: color-mix(in srgb, var(--aurora-color) 75%, transparent);
    top: -25%;
    left: -15%;
  }
  .second {
    --blob-color: color-mix(in srgb, color-mix(in srgb, var(--aurora-color) 55%, #3b82f6) 65%, transparent);
    top: 5%;
    right: -20%;
    animation-duration: 54s;
    animation-delay: -18s;
  }
  .third {
    --blob-color: color-mix(in srgb, color-mix(in srgb, var(--aurora-color) 50%, #a855f7) 60%, transparent);
    bottom: -35%;
    left: 20%;
    animation-duration: 63s;
    animation-delay: -31s;
  }
  .paused .blob {
    animation-play-state: paused;
  }
  /* Static noise dithers the gradients so dark backgrounds do not band. */
  .grain {
    position: absolute;
    inset: 0;
    opacity: 0.06;
    background-image: url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='160' height='160'%3E%3Cfilter id='n'%3E%3CfeTurbulence type='fractalNoise' baseFrequency='0.8' numOctaves='2' stitchTiles='stitch'/%3E%3CfeColorMatrix type='saturate' values='0'/%3E%3C/filter%3E%3Crect width='100%25' height='100%25' filter='url(%23n)'/%3E%3C/svg%3E");
  }
  @keyframes aurora-drift {
    from { transform: translate3d(-6%, -4%, 0) scale(1); }
    to { transform: translate3d(8%, 6%, 0) scale(1.12); }
  }
</style>
