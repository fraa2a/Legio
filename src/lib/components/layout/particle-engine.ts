export interface Particle {
  x: number;
  y: number;
  // CSS pixels per 60 Hz frame.
  vx: number;
  vy: number;
}

export type ParticleEdges = "bounce" | "wrap";

export function seedParticles(count: number, width: number, height: number, random: () => number, velocity: (random: () => number) => [number, number]): Particle[] {
  return Array.from({ length: count }, () => {
    const x = random() * width;
    const y = random() * height;
    const [vx, vy] = velocity(random);
    return { x, y, vx, vy };
  });
}

// Moves one particle by `scale` frames. Bouncing reverses at the edges; wrapping re-enters
// from the opposite side, `margin` pixels outside so large particles never pop in.
export function stepParticle(particle: Particle, width: number, height: number, edges: ParticleEdges, scale = 1, margin = 0): void {
  particle.x += particle.vx * scale;
  particle.y += particle.vy * scale;
  if (edges === "bounce") {
    if (particle.x < 0 || particle.x > width) particle.vx *= -1;
    if (particle.y < 0 || particle.y > height) particle.vy *= -1;
    return;
  }
  const spanX = width + margin * 2;
  const spanY = height + margin * 2;
  if (particle.x < -margin || particle.x > width + margin) particle.x = ((((particle.x + margin) % spanX) + spanX) % spanX) - margin;
  if (particle.y < -margin || particle.y > height + margin) particle.y = ((((particle.y + margin) % spanY) + spanY) % spanY) - margin;
}
