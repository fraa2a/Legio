<script lang="ts" generics="C">
  import { onMount } from "svelte";
  import { startBackgroundLoop, type BackgroundLoop, type CanvasRenderer } from "./background-loop";

  let { opacity, active = true, config, create, resolution = 1, maxFps = 60, pointer = false }: {
    opacity: number;
    active?: boolean;
    config: C;
    create: (canvas: HTMLCanvasElement) => CanvasRenderer<C> | null;
    resolution?: number;
    maxFps?: number;
    pointer?: boolean;
  } = $props();

  let container: HTMLDivElement;
  let canvas: HTMLCanvasElement;
  let loop = $state<BackgroundLoop<C>>();
  $effect(() => loop?.setActive(active));
  $effect(() => loop?.configure(config));

  onMount(() => {
    const renderer = create(canvas);
    if (renderer === null) return;
    const started = startBackgroundLoop({ container, canvas, renderer, config, active, resolution, maxFps, pointer });
    loop = started;
    return () => {
      loop = undefined;
      started.dispose();
    };
  });
</script>

<div bind:this={container} class="animated-background" style:opacity={opacity / 100} aria-hidden="true">
  <canvas bind:this={canvas}></canvas>
</div>
