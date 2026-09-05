/**
 * Confetti, as arithmetic.
 *
 * Pure so a test can prove the one thing that matters about it: every piece
 * leaves the screen, so the window that draws it can be put away again. A
 * burst that left one piece hovering would leave a full-screen window on top
 * of everything forever.
 */

export interface Particle {
  x: number;
  y: number;
  /** Pixels per second. */
  vx: number;
  vy: number;
  /** Long side, in pixels. */
  size: number;
  /** Radians. */
  angle: number;
  /** Radians per second. */
  spin: number;
  /** Which of the palette's colours. */
  colour: number;
}

/** Pixels per second squared. Heavier than paper so it is over in seconds. */
export const GRAVITY = 1400;

/** How much sideways speed survives each second. */
const DRAG = 0.35;

/**
 * A burst from the bottom corners, thrown upward and inward.
 *
 * Two fountains rather than a rain from the top, because rain is what a
 * window closing looks like and a fountain is what a celebration looks like.
 */
export function burst(
  width: number,
  height: number,
  count: number,
  random: () => number = Math.random,
): Particle[] {
  const pieces: Particle[] = [];

  for (let n = 0; n < count; n += 1) {
    const fromLeft = n % 2 === 0;
    const spread = (random() - 0.5) * 0.9;
    const speed = 900 + random() * 700;
    const angle = (fromLeft ? -Math.PI / 3 : (-2 * Math.PI) / 3) + spread;

    pieces.push({
      x: fromLeft ? 0 : width,
      y: height,
      vx: Math.cos(angle) * speed,
      vy: Math.sin(angle) * speed,
      size: 6 + random() * 8,
      angle: random() * Math.PI * 2,
      spin: (random() - 0.5) * 12,
      colour: Math.floor(random() * 4),
    });
  }

  return pieces;
}

/** Moves every piece on by `dt` seconds. */
export function step(pieces: Particle[], dt: number): void {
  const drag = Math.pow(DRAG, dt);

  for (const piece of pieces) {
    piece.vy += GRAVITY * dt;
    piece.vx *= drag;
    piece.x += piece.vx * dt;
    piece.y += piece.vy * dt;
    piece.angle += piece.spin * dt;
  }
}

/** Whether every piece is below the bottom edge, which is when to stop. */
export function settled(pieces: Particle[], height: number): boolean {
  return pieces.every((piece) => piece.y - piece.size > height);
}
