/**
 * Small repairs to what a model sends.
 */

/**
 * A space after a sentence that ran straight into the next.
 *
 * A turn that used a tool narrates before and after it, and the two halves
 * arrive glued: "...is real.Grabbing a few more...". The lowercase on both
 * flanks keeps decimals, paths and markdown out of reach, since `1.5`, `a.b`
 * and `**x**` all fail one side or the other.
 */
export function healRunOns(text: string): string {
  return text.replace(/([a-z][.!?:])([A-Z][a-z])/g, "$1 $2");
}

/** Seconds, said plainly, for how long something took. */
export function seconds(ms: number): string {
  if (ms < 1000) return "a moment";
  const s = ms / 1000;
  return s < 10 ? `${s.toFixed(1)} s` : `${Math.round(s)} s`;
}
