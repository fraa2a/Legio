<script lang="ts">
  import { onMount } from "svelte";
  import type { DitherSettings } from "../../services/appearance";
  import { createDitherRenderer } from "./dither-renderer";
  import { logApplicationEvent } from "../../services/application-log";

  let { opacity, settings, accent, active = true }: {
    opacity: number;
    settings: DitherSettings;
    accent: string;
    active?: boolean;
  } = $props();

  let container: HTMLDivElement;
  let canvas = $state<HTMLCanvasElement>();
  let update = $state<((enabled: boolean, configuration: DitherSettings, color: string) => void)>();
  $effect(() => update?.(active, settings, accent));

  function settingsKey(value: DitherSettings, color: string): string {
    return JSON.stringify([value.waveSpeed, value.waveFrequency, value.waveAmplitude, value.waveColor ?? color,
      value.backgroundColor, value.colorNum, value.pixelSize, value.disableAnimation, value.enableMouseInteraction, value.mouseRadius]);
  }
  let rendered = $state(false);
  onMount(() => {
    if (!canvas) return;
    const candidate = createDitherRenderer(canvas, diagnostic => {
      logApplicationEvent(diagnostic.event === "context_restored" ? "info" : "warn", "console", `Dither ${diagnostic.event}: ${diagnostic.detail}`);
      if (diagnostic.event === "context_lost") { rendered = false; cancelAnimationFrame(frame); frame = 0; }
      else if (diagnostic.event === "context_restored") { stale = true; schedule(); }
      else rendered = false;
    });
    if (!candidate) return;
    const renderer = candidate;
    const motion = window.matchMedia("(prefers-reduced-motion: reduce)");
    let width = 0;
    let height = 0;
    let ratio = 1;
    let time = 0;
    let frame = 0;
    let stale = true;
    let previousTime: number | null = null;
    let mouseX = -1e6;
    let mouseY = -1e6;

    function schedule(): void {
      if (active && frame === 0) frame = requestAnimationFrame(render);
    }

    function resize(): void {
      const bounds = container.getBoundingClientRect();
      ratio = Math.min(window.devicePixelRatio || 1, 2);
      const nextWidth = Math.max(1, Math.round(bounds.width * ratio));
      const nextHeight = Math.max(1, Math.round(bounds.height * ratio));
      if (nextWidth === width && nextHeight === height) return;
      width = nextWidth;
      height = nextHeight;
      rendered = false;
      renderer.resize(width, height);
      stale = true;
      schedule();
    }

    function render(now: number): void {
      frame = 0;
      if (!active) return;
      const frozen = settings.disableAnimation || motion.matches;
      if (!frozen && previousTime !== null) time += Math.min(now - previousTime, 100) / 1000;
      previousTime = frozen ? null : now;
      if (!frozen || stale) {
        stale = false;
        rendered = renderer.draw({ time, settings, accent, mouse: [mouseX, mouseY], ratio });
        if (!rendered) return;
      }
      if (!frozen) schedule();
    }

    function moved(event: PointerEvent): void {
      if (!active || !settings.enableMouseInteraction) return;
      const bounds = container.getBoundingClientRect();
      mouseX = (event.clientX - bounds.left) * ratio;
      mouseY = (event.clientY - bounds.top) * ratio;
      stale = true;
      schedule();
    }

    function motionChanged(): void {
      previousTime = null;
      stale = true;
      schedule();
    }

    const observer = new ResizeObserver(resize);
    observer.observe(container);
    window.addEventListener("pointermove", moved);
    motion.addEventListener("change", motionChanged);
    resize();
    let configurationKey = settingsKey(settings, accent);
    let enabledBefore = active;
    update = (enabled, configuration, color) => {
      const nextKey = settingsKey(configuration, color);
      if (nextKey === configurationKey && enabled === enabledBefore) return;
      configurationKey = nextKey; enabledBefore = enabled;
      cancelAnimationFrame(frame);
      frame = 0;
      previousTime = null;
      stale = true;
      if (enabled) schedule();
    };

    return () => {
      update = undefined; cancelAnimationFrame(frame); observer.disconnect();
      window.removeEventListener("pointermove", moved); motion.removeEventListener("change", motionChanged);
      renderer.dispose();
    };
  });
</script>

<div bind:this={container} class="animated-background" style:opacity={opacity / 100} style:background-color={settings.backgroundColor} aria-hidden="true">
  <canvas bind:this={canvas} style:visibility={rendered ? "visible" : "hidden"}></canvas>
</div>
