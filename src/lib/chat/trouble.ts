/**
 * What kind of failure a message describes, so the window can be calm about
 * the ones that are not the person's problem to fix.
 *
 * Two tiers rather than Dosage's three. Dosage retried a transport failure
 * silently, once, because a phone drops its connection between rooms; a
 * desktop launcher's failures are a service that is down or a key that is
 * wrong, and re-sending a question spends money nobody asked to spend.
 */

export type Tier = "limit" | "error";

/**
 * A limit is the service saying come back later. Nothing to retry now, and
 * nothing to alarm about: the sentence is enough.
 */
export function tier(why: string): Tier {
  const said = why.toLowerCase();

  if (
    said.includes("rate limit") ||
    said.includes("rate-limit") ||
    said.includes("rate limiting") ||
    said.includes("quota") ||
    said.includes("too many requests") ||
    said.includes("(429)")
  ) {
    return "limit";
  }

  return "error";
}
