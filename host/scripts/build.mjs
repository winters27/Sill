import { build, context } from "esbuild";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";
import { mkdir, writeFile } from "node:fs/promises";

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, "..");

const watch = process.argv.includes("--watch");

/**
 * Bundled to a single CommonJS file. The worker is spawned from this same file
 * via isMainThread, so it must stay one artifact: splitting it would break the
 * self-referencing Worker construction.
 */
const options = {
  entryPoints: [resolve(root, "src/index.ts")],
  outfile: resolve(root, "dist/host.js"),
  bundle: true,
  platform: "node",
  target: "node22",
  format: "cjs",
  sourcemap: true,
  // React and the reconciler are bundled in so extensions cannot resolve a
  // second copy. One React instance per worker is a hard requirement.
  external: [],
  logLevel: "info",
};

/**
 * A `type` marker beside the bundle.
 *
 * The output is CommonJS in a file called `.js`, so Node works out which it
 * is by walking up for the nearest `package.json`. Run where it is built
 * that finds `host/package.json`, which says `commonjs`, and it works.
 *
 * Copied out as a Tauri resource it does not. The bundle lands in
 * `src-tauri/target/<profile>/host/`, the nearest `package.json` above that
 * is the repository root's, and that one says `"type": "module"` for the
 * Svelte app. Node then parses the host as ESM and it dies on its first
 * `require` with not one line of it having run, which reads as every
 * extension being broken rather than as a packaging mistake.
 *
 * An installed copy is unaffected, because nothing above `Program Files` is
 * a package. That is what makes this the kind of break that only ever
 * happens on the machine it is being written on.
 */
await mkdir(resolve(root, "dist"), { recursive: true });
await writeFile(resolve(root, "dist/package.json"), '{ "type": "commonjs" }');

if (watch) {
  const ctx = await context(options);
  await ctx.watch();
  console.log("[sill-host] watching");
} else {
  await build(options);
}
