<script lang="ts">
  import { onMount } from "svelte";

  let { kind, opacity, color }: {
    kind: "particles" | "aurora";
    opacity: number;
    color: string;
  } = $props();

  let container: HTMLDivElement;
  let canvas = $state<HTMLCanvasElement>();

  onMount(() => {
    if (kind !== "particles" || !canvas) return;
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
      if (motion.matches) draw();
    }

    function draw(): void {
      context.clearRect(0, 0, width, height);
      const rgb = color.match(/[\da-f]{2}/gi)?.slice(0, 3).map((part) => Number.parseInt(part, 16)) ?? [130, 200, 220];
      const ink = `${rgb[0]}, ${rgb[1]}, ${rgb[2]}`;
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
          const distance = Math.hypot(particle.x - next.x, particle.y - next.y);
          if (distance > 140) continue;
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
      const bounds = container.getBoundingClientRect();
      pointer.x = event.clientX - bounds.left;
      pointer.y = event.clientY - bounds.top;
    }

    function motionChanged(): void {
      cancelAnimationFrame(frame);
      draw();
    }

    const observer = new ResizeObserver(resize);
    observer.observe(container);
    window.addEventListener("pointermove", moved);
    motion.addEventListener("change", motionChanged);
    resize();
    if (!motion.matches) frame = requestAnimationFrame(draw);
    return () => {
      cancelAnimationFrame(frame);
      observer.disconnect();
      window.removeEventListener("pointermove", moved);
      motion.removeEventListener("change", motionChanged);
    };
  });
</script>

<div bind:this={container} class="animated-background" style:opacity={opacity / 100} aria-hidden="true">
  {#if kind === "particles"}
    <canvas bind:this={canvas}></canvas>
  {:else}
    <div class="aurora aurora-one"></div>
    <div class="aurora aurora-two"></div>
    <div class="aurora aurora-three"></div>
  {/if}
</div>

<style>
  .animated-background { position: absolute; inset: 0; overflow: hidden; pointer-events: none; }
  canvas { width: 100%; height: 100%; }
  .aurora { position: absolute; width: 80%; height: 100%; border-radius: 50%; filter: blur(60px); mix-blend-mode: screen; animation: drift 18s ease-in-out infinite alternate; }
  .aurora-one { top: -45%; left: -20%; background: color-mix(in srgb, var(--legio-accent) 70%, transparent); }
  .aurora-two { right: -25%; bottom: -55%; background: color-mix(in srgb, var(--legio-accent) 45%, #004f86); animation-delay: -8s; }
  .aurora-three { top: 25%; left: 35%; width: 45%; height: 70%; background: color-mix(in srgb, var(--legio-accent) 35%, #761e80); animation-delay: -13s; }
  @keyframes drift { from { transform: translate(-8%, -4%) rotate(-12deg); } to { transform: translate(12%, 10%) rotate(18deg); } }
  @media (prefers-reduced-motion: reduce) { .aurora { animation: none; } }
</style>
