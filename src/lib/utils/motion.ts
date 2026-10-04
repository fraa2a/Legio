import type { TransitionConfig } from "svelte/transition";

export const reducedMotion = matchMedia("(prefers-reduced-motion: reduce)").matches;

export const fadeDuration = reducedMotion ? 0 : 110;

export const blurFadeDuration = reducedMotion ? 0 : 210;

export function blurFade(_node: HTMLElement, params: { duration?: number } = {}): TransitionConfig {
  return {
    duration: params.duration ?? blurFadeDuration,
    css: (progress) => `opacity:${progress};filter:blur(${(1 - progress) * 10}px);`,
  };
}
