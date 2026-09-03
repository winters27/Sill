/**
 * Sets Sill's version everywhere it is written down.
 *
 * `package.json` is the source and the rest are copies that a check refuses
 * to let drift, so bumping by hand is a six file edit that somebody gets
 * wrong once and then ships a build whose log header names a version that was
 * never released. This is that edit, done from one number.
 *
 * It does not commit and it does not tag. Tagging is what starts a release,
 * and the changelog has to be written between the bump and the tag.
 *
 * Run: node scripts/set-version.mjs 0.2.0
 */
import { readFileSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";

import { SOURCE, read, source, write } from "./versions.mjs";

const root = resolve(import.meta.dirname, "..");
const wanted = process.argv[2];

if (!wanted || !/^\d+\.\d+\.\d+(?:[-+].+)?$/.test(wanted)) {
  console.error("usage: node scripts/set-version.mjs <semver>, as in 0.2.0");
  process.exit(1);
}

const before = source();

// The source first, so a crash halfway through leaves the check failing
// rather than passing on the old number.
const packageJson = join(root, SOURCE);
const text = readFileSync(packageJson, "utf8");
const at = text.indexOf('"version":');
const end = text.indexOf("\n", at);
writeFileSync(
  packageJson,
  text.slice(0, at) +
    text.slice(at, end).replace(/"version":\s*"[^"]*"/, `"version": "${wanted}"`) +
    text.slice(end),
);

const touched = write(wanted);

console.log(`${before} -> ${wanted}`);
for (const file of [SOURCE, ...touched]) console.log(`  ${file}`);

const wrong = read().filter((copy) => copy.version !== wanted);
if (wrong.length) {
  for (const copy of wrong) {
    console.error(`  ${copy.file} still says ${copy.version ?? "nothing readable"}`);
  }
  process.exit(1);
}

console.log(
  "\nsrc-tauri/tauri.conf.json needs no edit: it reads package.json.\n" +
    "Next: write the CHANGELOG.md section for this version, commit, then tag.",
);
