export const reducedMotion = matchMedia("(prefers-reduced-motion: reduce)").matches;

// Durations follow the --legio-motion tokens in app.css.
export const fadeDuration = reducedMotion ? 0 : 120;

export const pageTransitionDuration = reducedMotion ? 0 : 200;

