const reducedMotion = matchMedia("(prefers-reduced-motion: reduce)").matches;

export const fadeDuration = reducedMotion ? 0 : 180;
