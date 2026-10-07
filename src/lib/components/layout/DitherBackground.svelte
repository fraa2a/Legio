<script lang="ts">
  import { onMount } from "svelte";
  import type { DitherSettings } from "../../services/appearance";
  import { ditherFragmentShader, waveFragmentShader, waveVertexShader } from "./dither-gl";

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

  function toRgb(color: string): [number, number, number] {
    const value = Number.parseInt(color.replace("#", ""), 16);
    if (!Number.isFinite(value)) return [0, 0, 0];
    return [((value >> 16) & 255) / 255, ((value >> 8) & 255) / 255, (value & 255) / 255];
  }

  onMount(() => {
    if (!canvas) return;
    const surface = canvas;
    const context = surface.getContext("webgl2", { alpha: false, antialias: false });
    if (context === null) {
      console.warn("WebGL2 is unavailable, the dither background was not rendered");
      return;
    }
    const gl = context;

    function compile(type: number, source: string): WebGLShader | null {
      const shader = gl.createShader(type);
      if (shader === null) return null;
      gl.shaderSource(shader, source);
      gl.compileShader(shader);
      if (gl.getShaderParameter(shader, gl.COMPILE_STATUS)) return shader;
      console.warn("Could not compile the dither shader", gl.getShaderInfoLog(shader));
      gl.deleteShader(shader);
      return null;
    }

    function link(fragmentSource: string): WebGLProgram | null {
      const vertex = compile(gl.VERTEX_SHADER, waveVertexShader);
      const fragment = compile(gl.FRAGMENT_SHADER, fragmentSource);
      if (vertex === null || fragment === null) {
        if (vertex !== null) gl.deleteShader(vertex);
        if (fragment !== null) gl.deleteShader(fragment);
        return null;
      }
      const handle = gl.createProgram();
      if (handle === null) {
        gl.deleteShader(vertex);
        gl.deleteShader(fragment);
        return null;
      }
      gl.attachShader(handle, vertex);
      gl.attachShader(handle, fragment);
      gl.bindAttribLocation(handle, 0, "position");
      gl.bindAttribLocation(handle, 1, "uv");
      gl.linkProgram(handle);
      gl.deleteShader(vertex);
      gl.deleteShader(fragment);
      if (gl.getProgramParameter(handle, gl.LINK_STATUS)) return handle;
      console.warn("Could not link the dither program", gl.getProgramInfoLog(handle));
      gl.deleteProgram(handle);
      return null;
    }

    function locate(handle: WebGLProgram, names: string[]): Record<string, WebGLUniformLocation | null> {
      const found: Record<string, WebGLUniformLocation | null> = {};
      for (const name of names) found[name] = gl.getUniformLocation(handle, name);
      return found;
    }

    const waveProgram = link(waveFragmentShader);
    const ditherProgram = link(ditherFragmentShader);
    if (waveProgram === null || ditherProgram === null) {
      if (waveProgram !== null) gl.deleteProgram(waveProgram);
      if (ditherProgram !== null) gl.deleteProgram(ditherProgram);
      return;
    }
    const wave = locate(waveProgram, [
      "resolution", "time", "waveSpeed", "waveFrequency", "waveAmplitude",
      "waveColor", "backgroundColor", "mousePos", "enableMouseInteraction", "mouseRadius",
    ]);
    const dither = locate(ditherProgram, ["inputBuffer", "resolution", "colorNum", "pixelSize"]);

    const quad = gl.createBuffer();
    gl.bindBuffer(gl.ARRAY_BUFFER, quad);
    gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([
      -1, -1, 0, 0, 0, 1, -1, 0, 1, 0, -1, 1, 0, 0, 1,
      -1, 1, 0, 0, 1, 1, -1, 0, 1, 0, 1, 1, 0, 1, 1,
    ]), gl.STATIC_DRAW);
    gl.enableVertexAttribArray(0);
    gl.vertexAttribPointer(0, 3, gl.FLOAT, false, 20, 0);
    gl.enableVertexAttribArray(1);
    gl.vertexAttribPointer(1, 2, gl.FLOAT, false, 20, 12);

    const texture = gl.createTexture();
    gl.bindTexture(gl.TEXTURE_2D, texture);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.NEAREST);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.NEAREST);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
    const framebuffer = gl.createFramebuffer();
    gl.bindFramebuffer(gl.FRAMEBUFFER, framebuffer);
    gl.framebufferTexture2D(gl.FRAMEBUFFER, gl.COLOR_ATTACHMENT0, gl.TEXTURE_2D, texture, 0);
    gl.bindFramebuffer(gl.FRAMEBUFFER, null);

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
      surface.width = width;
      surface.height = height;
      gl.bindTexture(gl.TEXTURE_2D, texture);
      gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, width, height, 0, gl.RGBA, gl.UNSIGNED_BYTE, null);
      stale = true;
      schedule();
    }

    function draw(): void {
      const waveColor = toRgb(settings.waveColor ?? accent);
      const backgroundColor = toRgb(settings.backgroundColor);
      gl.bindFramebuffer(gl.FRAMEBUFFER, framebuffer);
      gl.viewport(0, 0, width, height);
      gl.useProgram(waveProgram);
      gl.uniform2f(wave.resolution, width, height);
      gl.uniform1f(wave.time, time);
      gl.uniform1f(wave.waveSpeed, settings.waveSpeed);
      gl.uniform1f(wave.waveFrequency, settings.waveFrequency);
      gl.uniform1f(wave.waveAmplitude, settings.waveAmplitude);
      gl.uniform3f(wave.waveColor, waveColor[0], waveColor[1], waveColor[2]);
      gl.uniform3f(wave.backgroundColor, backgroundColor[0], backgroundColor[1], backgroundColor[2]);
      gl.uniform2f(wave.mousePos, mouseX, mouseY);
      gl.uniform1i(wave.enableMouseInteraction, settings.enableMouseInteraction ? 1 : 0);
      gl.uniform1f(wave.mouseRadius, settings.mouseRadius);
      gl.drawArrays(gl.TRIANGLES, 0, 6);

      gl.bindFramebuffer(gl.FRAMEBUFFER, null);
      gl.viewport(0, 0, width, height);
      gl.useProgram(ditherProgram);
      gl.activeTexture(gl.TEXTURE0);
      gl.bindTexture(gl.TEXTURE_2D, texture);
      gl.uniform1i(dither.inputBuffer, 0);
      gl.uniform2f(dither.resolution, width, height);
      gl.uniform1f(dither.colorNum, settings.colorNum);
      gl.uniform1f(dither.pixelSize, settings.pixelSize * ratio);
      gl.drawArrays(gl.TRIANGLES, 0, 6);
    }

    function render(now: number): void {
      frame = 0;
      if (!active) return;
      const frozen = settings.disableAnimation || motion.matches;
      if (!frozen && previousTime !== null) time += Math.min(now - previousTime, 100) / 1000;
      previousTime = frozen ? null : now;
      if (!frozen || stale) {
        stale = false;
        draw();
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
    update = (enabled) => {
      cancelAnimationFrame(frame);
      frame = 0;
      previousTime = null;
      stale = true;
      if (enabled) schedule();
    };

    return () => {
      update = undefined;
      cancelAnimationFrame(frame);
      observer.disconnect();
      window.removeEventListener("pointermove", moved);
      motion.removeEventListener("change", motionChanged);
      gl.deleteFramebuffer(framebuffer);
      gl.deleteTexture(texture);
      gl.deleteBuffer(quad);
      gl.deleteProgram(waveProgram);
      gl.deleteProgram(ditherProgram);
      const loseContext = gl.getExtension("WEBGL_lose_context") as WEBGL_lose_context | null;
      loseContext?.loseContext();
    };
  });
</script>

<div bind:this={container} class="animated-background" style:opacity={opacity / 100} aria-hidden="true">
  <canvas bind:this={canvas}></canvas>
</div>
