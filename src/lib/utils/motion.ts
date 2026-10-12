export const reducedMotion = matchMedia("(prefers-reduced-motion: reduce)").matches;

export const fadeDuration = reducedMotion ? 0 : 110;

export const pageTransitionDuration = reducedMotion ? 0 : 210;

