/**
 * The update endpoint, and the only place Sill is counted.
 *
 * Sill asks for `latest.json` on a schedule while somebody is using it. That
 * request already happened, to GitHub, from every install. This sits in front
 * of it so the checks can be counted, and **passes the answer through
 * unchanged**: GitHub stays the source of truth, nothing is uploaded here, and
 * there is no copy to go stale.
 *
 * ## What is stored, and what is not
 *
 * A daily identifier, and nothing else. It is
 * `sha256(ip + secret + the day)` cut to sixteen characters, which means:
 *
 * - It cannot be turned back into an address. There is no table anywhere that
 *   maps one to the other, here or at Cloudflare.
 * - **It changes at midnight**, because the day is inside the hash. Two rows
 *   from two days cannot be told to be the same machine, so this counts how
 *   many checked in on a day and can never follow one across days.
 * - No address is written down at any point. The hash is taken from the header
 *   and the string is discarded.
 *
 * Sill sends no identifier of its own. The client is unchanged apart from
 * which host it asks.
 *
 * ## Why an update must never depend on this
 *
 * Counting is the least important thing this file does. Every failure path
 * ends in the proxied answer being returned anyway, and the counting runs
 * after the response is on its way. `tauri.conf.json` also lists GitHub as a
 * second endpoint, so an outage here costs the count and nothing else.
 */

/** Where the real manifest lives. This never holds a copy of it. */
const SOURCE = "https://github.com/winters27/Sill/releases/latest/download/latest.json";

/**
 * How long an answer is reused before GitHub is asked again.
 *
 * Five minutes. A release is published rarely and checked often, so this is
 * almost entirely about not making a request to GitHub per check; the cost of
 * being five minutes late to a release nobody is waiting on is nothing.
 */
const CACHE_SECONDS = 300;

/**
 * How long a day's rows are kept.
 *
 * Long enough to draw a line on a chart, short enough that this is not a
 * growing record of anything. The rows cannot identify anybody either way.
 */
const KEEP_DAYS = 180;

/**
 * The one path this answers.
 *
 * The hostname is a custom domain, so every path on it reaches this worker.
 * Answering all of them would mean a crawler, a link preview or somebody
 * typing the address counted as a machine running Sill, which is the number
 * this exists to get right. Anything else is a 404 and is not counted.
 */
const PATH = "/latest.json";

export default {
  async fetch(request, env, ctx) {
    if (new URL(request.url).pathname !== PATH) {
      return new Response("Not found", { status: 404 });
    }

    // Asked for before anything else can fail, so the proxy is what happens
    // even when the counting cannot.
    const answer = proxy();

    // After the response, never in front of it. A check that takes longer
    // because it is being counted is the counter charging the user for it.
    ctx.waitUntil(count(request, env));

    return answer;
  },
};

/**
 * The manifest, from GitHub, unchanged.
 *
 * Byte for byte: the updater verifies a minisign signature against a public
 * key compiled into Sill, so anything rewritten here would fail that check on
 * every machine at once. It is also pointless to rewrite. The manifest names
 * GitHub for the installer itself, so the download never comes through here
 * and the bandwidth stays where it already was.
 */
function proxy() {
  return fetch(SOURCE, {
    cf: { cacheTtl: CACHE_SECONDS, cacheEverything: true },
  });
}

/**
 * Records that some machine checked today, without learning which.
 *
 * Every failure is swallowed. There is nothing a person could do about it and
 * nothing worth failing an update over.
 */
async function count(request, env) {
  try {
    if (!env.DB || !env.SALT) return;

    const day = new Date().toISOString().slice(0, 10);
    const id = await daily(request, env.SALT, day);
    if (!id) return;

    // The primary key does the work: a machine that checks forty times in a
    // day is one row, which is the whole point of counting this way.
    await env.DB.prepare("INSERT OR IGNORE INTO checks (day, id) VALUES (?, ?)")
      .bind(day, id)
      .run();

    await forget(env, day);
  } catch {
    // Deliberately silent. See the note above.
  }
}

/**
 * Today's identifier for whoever sent this request.
 *
 * The day is inside the hash rather than beside it, which is what stops the
 * same address producing the same identifier tomorrow. `CF-Connecting-IP` is
 * set by Cloudflare and cannot be spoofed by the client; without it there is
 * nothing to count and the request is skipped rather than counted as a
 * machine that does not exist.
 */
async function daily(request, salt, day) {
  const from = request.headers.get("CF-Connecting-IP");
  if (!from) return null;

  const bytes = new TextEncoder().encode(`${from}:${salt}:${day}`);
  const digest = await crypto.subtle.digest("SHA-256", bytes);

  return [...new Uint8Array(digest)]
    .slice(0, 8)
    .map((byte) => byte.toString(16).padStart(2, "0"))
    .join("");
}

/**
 * Drops rows past the retention window.
 *
 * On roughly one request in five hundred rather than on a schedule, so there
 * is no second thing to deploy and nothing runs when nobody is using Sill.
 * Which request does the tidying does not matter, only that one eventually
 * does.
 */
async function forget(env, day) {
  if (Math.random() > 0.002) return;

  const cutoff = new Date(Date.now() - KEEP_DAYS * 86_400_000).toISOString().slice(0, 10);
  if (cutoff >= day) return;

  await env.DB.prepare("DELETE FROM checks WHERE day < ?").bind(cutoff).run();
}
