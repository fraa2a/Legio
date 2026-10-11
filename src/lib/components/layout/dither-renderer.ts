import type { DitherSettings } from "../../services/appearance";
import { ditherFragmentShader, waveFragmentShader, waveVertexShader } from "./dither-gl";

export interface DitherFrame { time: number; settings: DitherSettings; accent: string; mouse: [number, number]; ratio: number }
export interface DitherDiagnostic { event: "context_lost" | "context_restored" | "framebuffer_incomplete" | "render_failed"; detail: string }
export interface DitherRenderer { resize(width: number, height: number): void; draw(frame: DitherFrame): boolean; dispose(): void }
function toRgb(color: string): [number, number, number] {
  const value = Number.parseInt(color.replace("#", ""), 16);
  if (!Number.isFinite(value)) return [0, 0, 0];
  return [((value >> 16) & 255) / 255, ((value >> 8) & 255) / 255, (value & 255) / 255];
}


export function createDitherRenderer(surface: HTMLCanvasElement, onDiagnostic: (event: DitherDiagnostic) => void): DitherRenderer | null {
  const context = surface.getContext("webgl2", { alpha: false, antialias: false, preserveDrawingBuffer: true });
  if (!context) { onDiagnostic({ event: "render_failed", detail: "WebGL2 unavailable" }); return null; }
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


  function allocate() {
    const waveProgram = link(waveFragmentShader);
    const ditherProgram = link(ditherFragmentShader);
    if (waveProgram === null || ditherProgram === null) {
      if (waveProgram !== null) gl.deleteProgram(waveProgram);
      if (ditherProgram !== null) gl.deleteProgram(ditherProgram);
      onDiagnostic({ event: "render_failed", detail: "Dither shader allocation failed" });
      return undefined;
    }
    const wave = locate(waveProgram, [
      "resolution", "time", "waveSpeed", "waveFrequency", "waveAmplitude",
      "waveColor", "backgroundColor", "mousePos", "enableMouseInteraction", "mouseRadius",
    ]);
    const dither = locate(ditherProgram, ["inputBuffer", "resolution", "colorNum", "pixelSize"]);

    const quad = gl.createBuffer();
    const texture = gl.createTexture();
    const framebuffer = gl.createFramebuffer();
    if (quad === null || texture === null || framebuffer === null) {
      gl.deleteBuffer(quad); gl.deleteTexture(texture); gl.deleteFramebuffer(framebuffer);
      gl.deleteProgram(waveProgram); gl.deleteProgram(ditherProgram);
      onDiagnostic({ event: "render_failed", detail: "Dither resource allocation failed" });
      return undefined;
    }
    gl.bindBuffer(gl.ARRAY_BUFFER, quad);
    gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([
      -1, -1, 0, 0, 0, 1, -1, 0, 1, 0, -1, 1, 0, 0, 1,
      -1, 1, 0, 0, 1, 1, -1, 0, 1, 0, 1, 1, 0, 1, 1,
    ]), gl.STATIC_DRAW);
    gl.enableVertexAttribArray(0);
    gl.vertexAttribPointer(0, 3, gl.FLOAT, false, 20, 0);
    gl.enableVertexAttribArray(1);
    gl.vertexAttribPointer(1, 2, gl.FLOAT, false, 20, 12);

    gl.bindTexture(gl.TEXTURE_2D, texture);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.NEAREST);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.NEAREST);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
    gl.bindFramebuffer(gl.FRAMEBUFFER, framebuffer);
    gl.framebufferTexture2D(gl.FRAMEBUFFER, gl.COLOR_ATTACHMENT0, gl.TEXTURE_2D, texture, 0);
    gl.bindFramebuffer(gl.FRAMEBUFFER, null);

    return { waveProgram, ditherProgram, wave, dither, quad, texture, framebuffer };
  }
  let resources = allocate();
  let width = 0; let height = 0; let lost = false; let complete = false; let disposed = false;
  function release(): void {
    if (!resources) return;
    gl.deleteFramebuffer(resources.framebuffer); gl.deleteTexture(resources.texture); gl.deleteBuffer(resources.quad);
    gl.deleteProgram(resources.waveProgram); gl.deleteProgram(resources.ditherProgram); resources = undefined;
  }
  function resize(nextWidth: number, nextHeight: number): void {
    if (disposed || (width === nextWidth && height === nextHeight && complete)) return;
    width = nextWidth; height = nextHeight; complete = false;
    if (lost || !resources) return;
    surface.width = width; surface.height = height;
    gl.bindTexture(gl.TEXTURE_2D, resources.texture);
    gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, width, height, 0, gl.RGBA, gl.UNSIGNED_BYTE, null);
    gl.bindFramebuffer(gl.FRAMEBUFFER, resources.framebuffer);
    const status = gl.checkFramebufferStatus(gl.FRAMEBUFFER);
    gl.bindFramebuffer(gl.FRAMEBUFFER, null);
    complete = status === gl.FRAMEBUFFER_COMPLETE;
    if (!complete) onDiagnostic({ event: "framebuffer_incomplete", detail: `Framebuffer status: ${status}` });
  }
  function contextLost(event: Event): void {
    event.preventDefault(); lost = true; complete = false; resources = undefined;
    onDiagnostic({ event: "context_lost", detail: "WebGL context lost" });
  }
  function contextRestored(): void {
    if (disposed) return;
    lost = false; resources = allocate(); resize(width, height);
    onDiagnostic({ event: "context_restored", detail: resources && complete ? "WebGL resources restored" : "WebGL recovery failed" });
  }
  surface.addEventListener("webglcontextlost", contextLost);
  surface.addEventListener("webglcontextrestored", contextRestored);
  return {
    resize,
    draw(frame) {
      if (disposed || lost || !complete || !resources || gl.isContextLost()) return false;
      const { waveProgram, ditherProgram, wave, dither, texture, framebuffer } = resources;
      const waveColor = toRgb(frame.settings.waveColor ?? frame.accent);
      const backgroundColor = toRgb(frame.settings.backgroundColor);
      gl.bindFramebuffer(gl.FRAMEBUFFER, framebuffer);
      gl.viewport(0, 0, width, height);
      gl.useProgram(waveProgram);
      gl.uniform2f(wave.resolution, width, height);
      gl.uniform1f(wave.time, frame.time);
      gl.uniform1f(wave.waveSpeed, frame.settings.waveSpeed);
      gl.uniform1f(wave.waveFrequency, frame.settings.waveFrequency);
      gl.uniform1f(wave.waveAmplitude, frame.settings.waveAmplitude);
      gl.uniform3f(wave.waveColor, waveColor[0], waveColor[1], waveColor[2]);
      gl.uniform3f(wave.backgroundColor, backgroundColor[0], backgroundColor[1], backgroundColor[2]);
      gl.uniform2f(wave.mousePos, frame.mouse[0], frame.mouse[1]);
      gl.uniform1i(wave.enableMouseInteraction, frame.settings.enableMouseInteraction ? 1 : 0);
      gl.uniform1f(wave.mouseRadius, frame.settings.mouseRadius);
      gl.drawArrays(gl.TRIANGLES, 0, 6);
      if (gl.isContextLost()) return false;

      gl.bindFramebuffer(gl.FRAMEBUFFER, null);
      gl.viewport(0, 0, width, height);
      gl.useProgram(ditherProgram);
      gl.activeTexture(gl.TEXTURE0);
      gl.bindTexture(gl.TEXTURE_2D, texture);
      gl.uniform1i(dither.inputBuffer, 0);
      gl.uniform2f(dither.resolution, width, height);
      gl.uniform1f(dither.colorNum, frame.settings.colorNum);
      gl.uniform1f(dither.pixelSize, frame.settings.pixelSize * frame.ratio);
      gl.drawArrays(gl.TRIANGLES, 0, 6);
      return !gl.isContextLost();
    },
    dispose() { if (disposed) return; disposed = true; surface.removeEventListener("webglcontextlost", contextLost); surface.removeEventListener("webglcontextrestored", contextRestored); release(); },
  };
}
