import { build } from "esbuild";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";
import { spawnSync } from "node:child_process";

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, "..");
const out = resolve(root, "dist/smoke.cjs");

await build({
  entryPoints: [resolve(root, "test/smoke.tsx")],
  outfile: out,
  bundle: true,
  platform: "node",
  target: "node22",
  format: "cjs",
  sourcemap: "inline",
  logLevel: "warning",
});

const res = spawnSync(process.execPath, [out], { stdio: "inherit" });
process.exit(res.status ?? 1);
