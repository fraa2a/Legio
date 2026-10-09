import type { TransitionConfig } from "svelte/transition";

export const reducedMotion = matchMedia("(prefers-reduced-motion: reduce)").matches;

export const fadeDuration = reducedMotion ? 0 : 110;

export const pageTransitionDuration = reducedMotion ? 0 : 210;

export function pageTransition(_node: HTMLElement, params: { duration?: number } = {}): TransitionConfig {
  return {
    duration: params.duration ?? pageTransitionDuration,
    css: (progress) => `transform:translateY(${(1 - progress) * 6}px);`,
  };
}
