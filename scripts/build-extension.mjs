/**
 * Builds one Raycast extension command into something Sill can load.
 *
 * This is what `ray build` does, minus everything tied to Raycast's own
 * toolchain: bundle the command's entrypoint to CommonJS with `@raycast/api`
 * and `react` left external, because the host supplies both at runtime and a
 * second React instance in the worker would break hooks.
 *
 * Preview of M8. Kept deliberately small so the moving parts stay visible.
 *
 * Usage: node scripts/build-extension.mjs <extension-dir> [command-name]
 *          [--watch [-- <run-extension.mjs flags>]]
 */
import { context } from "../host/node_modules/esbuild/lib/main.js";
import { spawn } from "node:child_process";
import { existsSync, readFileSync, mkdirSync, writeFileSync } from "node:fs";
import { join, resolve, dirname } from "node:path";

/*
 * Everything after a bare `--` belongs to the run, not to the build.
 *
 * Split before anything is read, so a flag meant for `run-extension.mjs`
 * cannot be mistaken for the command name. `--grant` and `--on` are the two
 * that matter in practice: an extension being written usually needs a
 * permission and, if it contributes an action, something to act on.
 */
const argv = process.argv.slice(2);
const separator = argv.indexOf("--");
const mine = separator === -1 ? argv : argv.slice(0, separator);
const forTheRun = separator === -1 ? [] : argv.slice(separator + 1);

const watching = mine.includes("--watch");
const [extensionDir, requestedCommand] = mine.filter((one) => !one.startsWith("--"));

if (!extensionDir) {
  console.error(
    "usage: node scripts/build-extension.mjs <extension-dir> [command-name] " +
      "[--watch [-- <run-extension.mjs flags>]]",
  );
  process.exit(1);
}

const root = resolve(import.meta.dirname, "..");
const extRoot = resolve(extensionDir);
const manifestPath = join(extRoot, "package.json");

if (!existsSync(manifestPath)) {
  console.error(`no package.json at ${manifestPath}`);
  process.exit(1);
}

const manifest = JSON.parse(readFileSync(manifestPath, "utf8"));
const commands = manifest.commands ?? [];

if (commands.length === 0) {
  console.error(`${manifest.name} declares no commands`);
  process.exit(1);
}

const command = requestedCommand
  ? commands.find((c) => c.name === requestedCommand)
  : commands.find((c) => c.mode === "view") ?? commands[0];

if (!command) {
  console.error(
    `no such command "${requestedCommand}". Available: ${commands.map((c) => c.name).join(", ")}`,
  );
  process.exit(1);
}

/** Commands map to src/<name> with any of the usual extensions. */
function entrypointFor(name) {
  for (const ext of [".tsx", ".ts", ".jsx", ".js"]) {
    const candidate = join(extRoot, "src", `${name}${ext}`);
    if (existsSync(candidate)) return candidate;
  }
  throw new Error(`no source file for command "${name}" under ${join(extRoot, "src")}`);
}

/**
 * Extensions commonly alias "@/..." to their own src. esbuild does not read
 * tsconfig paths for bare aliases reliably across versions, so they are lifted
 * out of tsconfig and passed explicitly.
 */
function aliasesFromTsconfig() {
  const tsconfigPath = join(extRoot, "tsconfig.json");
  if (!existsSync(tsconfigPath)) return {};

  let parsed;
  try {
    // tsconfig files allow comments, which JSON.parse rejects.
    const text = readFileSync(tsconfigPath, "utf8").replace(/^\s*\/\/.*$/gm, "");
    parsed = JSON.parse(text);
  } catch {
    return {};
  }

  const paths = parsed?.compilerOptions?.paths ?? {};
  const baseUrl = parsed?.compilerOptions?.baseUrl ?? ".";
  const out = {};

  for (const [pattern, targets] of Object.entries(paths)) {
    const target = Array.isArray(targets) ? targets[0] : targets;
    if (!target) continue;
    out[pattern.replace(/\/\*$/, "")] = resolve(extRoot, baseUrl, target.replace(/\/\*$/, ""));
  }

  return out;
}

const entry = entrypointFor(command.name);
const outfile = join(root, "extensions", "build", manifest.name, `${command.name}.js`);
mkdirSync(dirname(outfile), { recursive: true });

// Sill's own package.json declares "type": "module", and that scope reaches
// down into the build directory, so Node would load these CommonJS bundles as
// ES modules and fail on `module`. A scope marker beside the output pins them
// back to CommonJS, which is what every Raycast extension bundle is.
writeFileSync(
  join(dirname(outfile), "package.json"),
  `${JSON.stringify({ type: "commonjs", private: true }, null, 2)}\n`,
);

const options = {
  entryPoints: [entry],
  outfile,
  bundle: true,
  platform: "node",
  format: "cjs",
  target: "node20",
  jsx: "automatic",
  jsxImportSource: "react",
  // Supplied by the host. Bundling either would give the worker a second copy
  // of React, and hooks would fail in ways that look like extension bugs.
  external: ["@raycast/api", "@sill/api", "react", "react/jsx-runtime", "react/jsx-dev-runtime"],
  alias: aliasesFromTsconfig(),
  logLevel: "warning",
  sourcemap: false,
  minify: false,
};

/**
 * The build, once, or the loop.
 *
 * `context` either way rather than `build` for one and `context` for the
 * other, so a single build and a watched one are the same esbuild
 * configuration reaching the same disk. Two code paths through a bundler is
 * how a watch loop ends up producing something the one-shot build does not.
 */
const ctx = await context(options);
await ctx.rebuild();

if (!watching) await ctx.dispose();

/**
 * What `getPreferenceValues()` should return before anyone has changed
 * anything.
 *
 * A manifest declares preferences at the extension level and again per
 * command, and a command sees both with its own winning. Nothing was reading
 * either, so every extension ran with an empty object: `prefs.defaultAction`
 * came back undefined in code that had no reason to expect it, which is the
 * kind of failure that reads as an extension bug.
 *
 * Only preferences that declare a default appear. One that does not is
 * genuinely unset until a person sets it, and inventing a value would be
 * worse than the undefined the extension already guards against.
 */
function defaultPreferences(manifest, command) {
  const collected = {};
  for (const list of [manifest.preferences ?? [], command.preferences ?? []]) {
    for (const preference of list) {
      if (preference?.name && preference.default !== undefined) {
        collected[preference.name] = preference.default;
      }
    }
  }
  return collected;
}

const record = {
  id: `${manifest.name}:${command.name}`,
  extension: manifest.name,
  extensionTitle: manifest.title ?? manifest.name,
  command: command.name,
  title: command.title ?? command.name,
  subtitle: command.subtitle ?? manifest.title ?? manifest.name,
  description: command.description ?? "",
  mode: command.mode,
  entrypoint: outfile.replace(/\\/g, "/"),
  keywords: command.keywords ?? [],
  preferences: defaultPreferences(manifest, command),
  /*
   * What the command contributes, carried into the index Sill reads.
   *
   * Without this an extension built here draws its action nowhere, and the
   * only way to see one work would be to install the extension properly. That
   * is exactly the loop this script exists to shorten: `Declared` takes
   * defaults for every field it does not get, so one key is the whole of it.
   */
  manifest: { actsOn: command.sill?.actionOn ?? [] },
};

// The registry Rust reads at startup. Rewritten rather than appended so a
// rebuilt command updates in place instead of appearing twice.
const indexPath = join(root, "extensions", "build", "index.json");
let index = [];
if (existsSync(indexPath)) {
  try {
    index = JSON.parse(readFileSync(indexPath, "utf8"));
  } catch {
    // A corrupt index is not worth failing a build over; it gets replaced.
    index = [];
  }
}

index = index.filter((entry) => entry.id !== record.id);
index.push(record);
index.sort((a, b) => a.id.localeCompare(b.id));
writeFileSync(indexPath, `${JSON.stringify(index, null, 2)}\n`);

console.log(JSON.stringify(record, null, 2));
console.log(`\nindex now lists ${index.length} command(s): ${indexPath}`);

if (!watching) process.exit(0);

/*
 * The loop, which is the whole of what a watch mode is for.
 *
 * Rebuild, then run the result against the real host and print what it drew.
 * Building without running would be the less useful half: esbuild reports a
 * syntax error either way, and what somebody writing an extension actually
 * wants to see is the list, the toast or the gap the change produced.
 *
 * One run at a time, and a change that lands while one is in flight kills it.
 * A watch that queues runs turns a burst of saves into a queue of stale
 * answers, each printed after the edit that made it wrong.
 */
const runner = resolve(import.meta.dirname, "run-extension.mjs");
let running;

function run() {
  if (running) {
    // Its own output is about to be replaced by a newer one, so nothing is
    // lost by ending it here.
    running.kill();
    running = undefined;
  }

  console.log(`\n--- ${new Date().toLocaleTimeString()} ${manifest.name}/${command.name} ---`);

  running = spawn(
    process.execPath,
    [runner, outfile, manifest.name, ...forTheRun],
    { stdio: "inherit" },
  );

  running.on("exit", () => {
    running = undefined;
    console.log("\nwatching. Save the extension again, or press Ctrl+C.");
  });
}

await ctx.watch();
run();

/*
 * esbuild's watcher rebuilds on its own and says nothing this script can hook
 * without a plugin, so the run is triggered by the same thing esbuild is
 * watching: the entry point and whatever it pulled in. `fs.watch` on the
 * output is the honest signal, because a rebuild that produced no change
 * writes nothing and a run then would be a run for no reason.
 */
const { watch } = await import("node:fs");
let settling;

watch(dirname(outfile), (_event, name) => {
  if (name !== `${command.name}.js`) return;

  // Editors and bundlers both write in bursts, and a rebuilt bundle lands as
  // several events. One run per burst.
  clearTimeout(settling);
  settling = setTimeout(run, 120);
});

for (const signal of ["SIGINT", "SIGTERM"]) {
  process.on(signal, () => {
    running?.kill();
    void ctx.dispose();
    process.exit(0);
  });
}
