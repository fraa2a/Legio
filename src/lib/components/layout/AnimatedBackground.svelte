<script lang="ts">
  import { onMount } from "svelte";

  let { opacity, color, active = true }: {
    opacity: number;
    color: string;
    active?: boolean;
  } = $props();

  let container: HTMLDivElement;
  let canvas = $state<HTMLCanvasElement>();
  let update = $state<((enabled: boolean) => void)>();
  $effect(() => update?.(active));

  onMount(() => {
    if (!canvas) return;
    const surface = canvas;
    const maybeContext = surface.getContext("2d");
    if (!maybeContext) return;
    const context: CanvasRenderingContext2D = maybeContext;

    const motion = window.matchMedia("(prefers-reduced-motion: reduce)");
    const particles: { x: number; y: number; vx: number; vy: number }[] = [];
    const pointer = { x: -1000, y: -1000 };
    let width = 0;
    let height = 0;
    let frame = 0;
    const rgb = color.match(/[\da-f]{2}/gi)?.slice(0, 3).map((part) => Number.parseInt(part, 16)) ?? [130, 200, 220];
    const ink = `${rgb[0]}, ${rgb[1]}, ${rgb[2]}`;

    function resize(): void {
      const bounds = container.getBoundingClientRect();
      width = bounds.width;
      height = bounds.height;
      const ratio = Math.min(window.devicePixelRatio || 1, 2);
      surface.width = Math.round(width * ratio);
      surface.height = Math.round(height * ratio);
      context.setTransform(ratio, 0, 0, ratio, 0, 0);
      particles.length = 0;
      const count = Math.max(12, Math.min(48, Math.round(width * height / 14000)));
      for (let index = 0; index < count; index += 1) {
        particles.push({
          x: Math.random() * width, y: Math.random() * height,
          vx: (Math.random() - 0.5) * 0.42, vy: (Math.random() - 0.5) * 0.42,
        });
      }
      if (active && motion.matches) draw();
    }

    function draw(): void {
      frame = 0;
      if (!active) return;
      context.clearRect(0, 0, width, height);
      for (let index = 0; index < particles.length; index += 1) {
        const particle = particles[index];
        if (!motion.matches) {
          particle.x += particle.vx;
          particle.y += particle.vy;
          if (particle.x < 0 || particle.x > width) particle.vx *= -1;
          if (particle.y < 0 || particle.y > height) particle.vy *= -1;
        }
        for (let other = index + 1; other < particles.length; other += 1) {
          const next = particles[other];
          const squared = (particle.x - next.x) ** 2 + (particle.y - next.y) ** 2;
          if (squared > 140 ** 2) continue;
          const distance = Math.sqrt(squared);
          context.strokeStyle = `rgba(${ink}, ${0.35 * (1 - distance / 140)})`;
          context.beginPath();
          context.moveTo(particle.x, particle.y);
          context.lineTo(next.x, next.y);
          context.stroke();
        }
        const pointerDistance = Math.hypot(particle.x - pointer.x, particle.y - pointer.y);
        if (pointerDistance < 180) {
          context.strokeStyle = `rgba(${ink}, ${0.5 * (1 - pointerDistance / 180)})`;
          context.beginPath();
          context.moveTo(particle.x, particle.y);
          context.lineTo(pointer.x, pointer.y);
          context.stroke();
        }
        context.fillStyle = `rgba(${ink}, 0.8)`;
        context.beginPath();
        context.arc(particle.x, particle.y, 1.7, 0, Math.PI * 2);
        context.fill();
      }
      if (!motion.matches) frame = requestAnimationFrame(draw);
    }

    function moved(event: PointerEvent): void {
      if (!active) return;
      const bounds = container.getBoundingClientRect();
      pointer.x = event.clientX - bounds.left;
      pointer.y = event.clientY - bounds.top;
      if (motion.matches) draw();
    }

    function motionChanged(): void {
      cancelAnimationFrame(frame);
      frame = 0;
      draw();
    }

    const observer = new ResizeObserver(resize);
    observer.observe(container);
    window.addEventListener("pointermove", moved);
    motion.addEventListener("change", motionChanged);
    resize();
    update = (enabled) => {
      cancelAnimationFrame(frame);
      frame = 0;
      if (enabled) draw();
    };
    return () => {
      update = undefined;
      cancelAnimationFrame(frame);
      observer.disconnect();
      window.removeEventListener("pointermove", moved);
      motion.removeEventListener("change", motionChanged);
    };
  });
</script>

<div bind:this={container} class="animated-background" style:opacity={opacity / 100} aria-hidden="true">
  <canvas bind:this={canvas}></canvas>
</div>
