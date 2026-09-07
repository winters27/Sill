/**
 * Waits for `verify` to finish on one commit, and says whether it passed.
 *
 * The release build and `verify` start from the same tag push and run beside
 * each other. The build takes the best part of an hour and `verify` a quarter
 * of it, so by the time there are installers to publish the answer is almost
 * always already there. Almost: a cold Rust cache, or a runner queue, can put
 * `verify` behind, and publishing a release whose tests never finished is the
 * one outcome this whole pipeline exists to prevent.
 *
 * Exits 0 when it passed, non-zero for anything else, including never having
 * started. A release that cannot be shown to be green stays a draft, which is
 * a person's decision to make rather than a build's.
 *
 * ```bash
 * node scripts/wait-for-verify.mjs winters27/Sill "$GITHUB_SHA"
 * ```
 */
import { execFileSync } from "node:child_process";
import { setTimeout as sleep } from "node:timers/promises";

const [repository, sha] = process.argv.slice(2);

if (!repository || !sha) {
  console.error("usage: node scripts/wait-for-verify.mjs <owner/repo> <sha>");
  process.exit(2);
}

/** How long to wait in total, and how often to ask. */
const GIVE_UP_AFTER = 45 * 60 * 1000;
const ASK_EVERY = 30 * 1000;

/**
 * The runs of `verify` for this commit, newest first.
 *
 * Through `gh` rather than `fetch`, because the token this needs is the one
 * `gh` is already holding in the step's environment, and a second way to
 * authenticate is a second thing to get wrong.
 */
function runs() {
  const said = execFileSync(
    "gh",
    [
      "api",
      `repos/${repository}/actions/workflows/verify.yml/runs?head_sha=${sha}&per_page=20`,
      "--jq",
      "[.workflow_runs[] | {status, conclusion, url: .html_url}]",
    ],
    { encoding: "utf8" },
  );
  return JSON.parse(said);
}

const startedAt = Date.now();

while (Date.now() - startedAt < GIVE_UP_AFTER) {
  let found;
  try {
    found = runs();
  } catch (err) {
    // A transient API failure is not an answer about the tests. Asking again
    // is, and the deadline below is what stops this going on forever.
    console.log(`could not ask: ${err.message.trim()}`);
    await sleep(ASK_EVERY);
    continue;
  }

  const done = found.filter((run) => run.status === "completed");

  if (done.length > 0) {
    const [newest] = done;
    console.log(`verify ${newest.conclusion}: ${newest.url}`);
    process.exit(newest.conclusion === "success" ? 0 : 1);
  }

  const waited = Math.round((Date.now() - startedAt) / 1000);
  console.log(
    found.length === 0
      ? `no verify run for ${sha.slice(0, 10)} yet, ${waited}s`
      : `verify is ${found[0].status}, ${waited}s`,
  );
  await sleep(ASK_EVERY);
}

console.error(`::error::verify did not finish for ${sha} within 45 minutes`);
process.exit(1);
