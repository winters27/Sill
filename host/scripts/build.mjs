import { build, context } from "esbuild";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";

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

if (watch) {
  const ctx = await context(options);
  await ctx.watch();
  console.log("[sill-host] watching");
} else {
  await build(options);
}
