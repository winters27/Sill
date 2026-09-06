/**
 * Points the updater manifest at download links rather than API links.
 *
 * `tauri-action` writes `latest.json` while the release is still a draft, and a
 * draft's assets have no public download link, so it falls back to
 * `api.github.com/.../releases/assets/<id>`. Those work, and they are rate
 * limited: sixty unauthenticated requests an hour for a whole address. Two
 * people behind one connection, or one person and anything else on the network
 * that talks to GitHub, and an update fails with a 403 that names nothing a
 * person could act on.
 *
 * The download link has no such limit. It only resolves once the release is
 * published, which is fine: nothing reads the manifest before then.
 *
 * ```bash
 * gh release view v1.2.3 --json assets > assets.json
 * gh release download v1.2.3 --pattern latest.json
 * node scripts/manifest-urls.mjs latest.json assets.json winters27/Sill v1.2.3
 * gh release upload v1.2.3 latest.json --clobber
 * ```
 *
 * Rewrites in place and prints what it changed. Exits non-zero if a URL cannot
 * be matched to an asset, because a manifest pointing at the wrong file is
 * worse than one pointing at a rate-limited address.
 */
import { readFileSync, writeFileSync } from "node:fs";

const [manifestPath, assetsPath, repository, tag] = process.argv.slice(2);

if (!manifestPath || !assetsPath || !repository || !tag) {
  console.error(
    "usage: node scripts/manifest-urls.mjs <latest.json> <assets.json> <owner/repo> <tag>",
  );
  process.exit(2);
}

const manifest = JSON.parse(readFileSync(manifestPath, "utf8"));
const listed = JSON.parse(readFileSync(assetsPath, "utf8"));

// `gh release view --json assets` wraps them; a bare array is accepted too.
const assets = Array.isArray(listed) ? listed : listed.assets;
if (!Array.isArray(assets) || assets.length === 0) {
  console.error(`no assets in ${assetsPath}`);
  process.exit(1);
}

/** Asset id to file name, which is the only thing the API URL carries. */
const named = new Map();
for (const asset of assets) {
  // `id` is a number in the REST API and an opaque node id in `gh --json`.
  // Both are present in a full listing; the numeric one is what a URL holds.
  for (const key of ["id", "databaseId"]) {
    if (typeof asset[key] === "number") named.set(String(asset[key]), asset.name);
  }
  const fromUrl = String(asset.url ?? "").match(/\/assets\/(\d+)$/);
  if (fromUrl) named.set(fromUrl[1], asset.name);
}

const base = `https://github.com/${repository}/releases/download/${tag}/`;
let changed = 0;
const unmatched = [];

for (const [platform, entry] of Object.entries(manifest.platforms ?? {})) {
  const id = String(entry.url ?? "").match(/\/releases\/assets\/(\d+)$/);
  if (!id) {
    // Already a download link, which is the case on a release that was not a
    // draft when the manifest was written.
    continue;
  }

  const name = named.get(id[1]);
  if (!name) {
    unmatched.push(`${platform} points at asset ${id[1]}, which this release does not have`);
    continue;
  }

  entry.url = base + encodeURIComponent(name);
  console.log(`  ${platform} -> ${entry.url}`);
  changed += 1;
}

if (unmatched.length) {
  for (const why of unmatched) console.error(`  ${why}`);
  process.exit(1);
}

if (changed === 0) {
  console.log("every platform already points at a download link");
  process.exit(0);
}

writeFileSync(manifestPath, `${JSON.stringify(manifest, null, 2)}\n`);
console.log(`${changed} url(s) rewritten in ${manifestPath}`);
