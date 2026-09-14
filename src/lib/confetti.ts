/**
 * Confetti, as arithmetic.
 *
 * Pure so a test can prove the two things that matter about it: every piece
 * leaves the screen, so the window that draws it can be put away again, and
 * it does that in about the same time whatever the screen is. A burst that
 * left one piece hovering would leave a full-screen window on top of
 * everything forever.
 *
 * ## Everything here is measured in screens, not in pixels
 *
 * The first version was tuned in pixels per second against one display, and a
 * display is exactly the wrong thing to tune against. The same 1400px/s
 * gravity and 900px/s throw that fill a 768px laptop reach a third of the way
 * up a 1440p monitor and a fifth of the way up a 4K one, so the burst got
 * weaker the bigger the screen was and the fall took longer.
 *
 * So the constants below are fractions of the screen, and `burst` multiplies
 * them out once and stores each piece's own `gravity` and `terminal`. A piece
 * thrown nine tenths of the way up takes the same time to get there and the
 * same time to come down on any display, because the throw and the thing
 * pulling it back were scaled by the same number.
 *
 * ## Why a piece of paper is not a rotating rectangle
 *
 * Paper tumbles about its long axis, so it is edge-on twice a turn and
 * briefly invisible; it shows a darker side when it faces away; and it slides
 * sideways when it is flat to the air rather than falling straight.
 *
 * All three come from one number. `flip` is a cosine of how far a piece has
 * travelled, and it is used three ways: as the vertical squash, as which of
 * the two colours is facing, and as the direction of the sideways push. They
 * agree because they are the same value, which is what makes the motion read
 * as one object rather than as three effects.
 *
 * Adapted from StreamNook's `ConfettiBurst`, whose per-frame constants assume
 * 60fps. Everything here is per second, so a 144Hz screen does not run the
 * burst at twice the speed.
 */

export interface Particle {
  x: number;
  y: number;
  /** Pixels per second. */
  vx: number;
  vy: number;
  /** The long side of a piece of paper, or a sequin's diameter. */
  size: number;
  /** The short side. A sequin ignores it. */
  width: number;
  /** Radians. */
  angle: number;
  /** Radians per second. */
  spin: number;
  /** Which of the palette's colours. */
  colour: number;
  /** Pixels per second squared, scaled to the screen it was thrown on. */
  gravity: number;
  /** The fastest it falls, in pixels per second. Infinite for a sequin. */
  terminal: number;
  /** Radians of tumble per pixel fallen, so the flutter reads the same size. */
  swing: number;
  /**
   * Where this piece is in its own tumble, in pixels.
   *
   * Without it every piece at the same height is edge-on at the same instant
   * and the whole burst blinks in unison, which no amount of other variation
   * hides.
   */
  seed: number;
  /** Paper tumbles and flashes its back. A sequin is a disc that falls. */
  paper: boolean;
}

/** Screen heights per second squared. */
const GRAVITY = 1.3;

/**
 * The fastest paper falls, in screen heights per second.
 *
 * Paper reaches a speed and stays there, and the cap is most of why the fall
 * reads as fluttering rather than dropping. Without it a piece that launched
 * hard is a vertical streak by the time it reaches the bottom, moving too
 * fast for the tumble to be visible at all.
 */
const TERMINAL = 0.95;

/** A sequin is small and dense, so it outruns the paper and lands first. */
const GRAVITY_SEQUIN = 2.1;

/** How much sideways speed survives each second. */
const DRAG = 0.35;

/** A sequin keeps its sideways throw; it has no face to catch the air. */
const DRAG_SEQUIN = 0.8;

/**
 * Those two drags as rates, which is what turns a distance into a throw.
 *
 * Sideways speed decays as `v0 * DRAG^t`, so the whole distance a piece ever
 * covers is `v0 / -ln(DRAG)`. Aiming at a distance and dividing is the only
 * way to place the fountains: choosing a speed instead means the distance
 * moves silently every time the drag is touched.
 */
const SPEND = -Math.log(DRAG);
const SPEND_SEQUIN = -Math.log(DRAG_SEQUIN);

/** Sideways push at full tilt, as a share of the piece's own gravity. */
const FLUTTER = 0.45;

/** Turns of tumble over one screen height, which is what sets the flutter. */
const TURNS = 3;

/**
 * How far through its tumble a piece is: 1 face-on, 0 edge-on, -1 backwards.
 *
 * Exported because the drawing needs the same number the physics used, and
 * recomputing it there from a slightly different formula is how the squash
 * and the sway end up disagreeing.
 */
export function flip(piece: Particle): number {
  return Math.cos((piece.y + piece.seed) * piece.swing);
}

/**
 * A burst from the bottom corners, thrown upward and inward.
 *
 * Two fountains rather than a rain from the top, because rain is what a
 * window closing looks like and a fountain is what a celebration looks like.
 *
 * Every piece is aimed rather than handed a speed: how far up it should
 * reach and how far across, with the arithmetic to turn each into a velocity.
 * The weakest still clears half the height and the strongest goes over the
 * top, on a small window or a 4K screen.
 *
 * One piece in five is a sequin. They are thrown from the same corners and
 * fall faster, so they arrive ahead of the paper and the burst has something
 * happening at both ends of it rather than one flat front of rectangles.
 */
export function burst(
  width: number,
  height: number,
  count: number,
  random: () => number = Math.random,
): Particle[] {
  const pieces: Particle[] = [];
  const swing = (TURNS * 2 * Math.PI) / height;

  for (let n = 0; n < count; n += 1) {
    const fromLeft = n % 2 === 0;
    const paper = n % 5 !== 0;
    const gravity = (paper ? GRAVITY : GRAVITY_SEQUIN) * height;

    // How high this one goes. Half the screen at worst, over the top at best.
    const reach = height * (0.55 + random() * 0.62);
    // How far across it gets, once drag has spent the throw.
    const across = width * (0.22 + random() * 0.55);
    const long = paper ? 10 + random() * 10 : 3 + random() * 3;

    pieces.push({
      x: fromLeft ? 0 : width,
      y: height,
      vx: (fromLeft ? 1 : -1) * across * (paper ? SPEND : SPEND_SEQUIN),
      vy: -Math.sqrt(2 * gravity * reach),
      size: long,
      // Paper is oblong. A square tumbles into a square and the flip is only
      // a change of colour rather than a change of shape.
      width: paper ? long * (0.45 + random() * 0.25) : long,
      angle: random() * Math.PI * 2,
      spin: (random() - 0.5) * 6,
      colour: Math.floor(random() * 4),
      gravity,
      terminal: paper ? TERMINAL * height : Number.POSITIVE_INFINITY,
      swing,
      // A phase anywhere in one full tumble.
      seed: random() * ((Math.PI * 2) / swing),
      paper,
    });
  }

  return pieces;
}

/** Moves every piece on by `dt` seconds. */
export function step(pieces: Particle[], dt: number): void {
  const drag = Math.pow(DRAG, dt);
  const dragSequin = Math.pow(DRAG_SEQUIN, dt);

  for (const piece of pieces) {
    // `min`, not a clamp: while a piece is still rising `vy` is negative and
    // the cap must not pull it back down to terminal velocity.
    piece.vy = Math.min(piece.vy + piece.gravity * dt, piece.terminal);

    if (piece.paper) {
      piece.vx *= drag;
      // Flat to the air, so it slides; edge-on, so it does not. Same number
      // the squash is drawn from, which is what ties the two together.
      piece.vx += flip(piece) * piece.gravity * FLUTTER * dt;
      piece.angle += piece.spin * dt;
    } else {
      piece.vx *= dragSequin;
    }

    piece.x += piece.vx * dt;
    piece.y += piece.vy * dt;
  }
}

/** Whether every piece is below the bottom edge, which is when to stop. */
export function settled(pieces: Particle[], height: number): boolean {
  return pieces.every((piece) => piece.y - piece.size > height);
}
