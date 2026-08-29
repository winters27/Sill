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
 */
import { build } from "../host/node_modules/esbuild/lib/main.js";
import { existsSync, readFileSync, mkdirSync, writeFileSync } from "node:fs";
import { join, resolve, dirname } from "node:path";

const [, , extensionDir, requestedCommand] = process.argv;

if (!extensionDir) {
  console.error("usage: node scripts/build-extension.mjs <extension-dir> [command-name]");
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

await build({
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
  external: ["@raycast/api", "react", "react/jsx-runtime", "react/jsx-dev-runtime"],
  alias: aliasesFromTsconfig(),
  logLevel: "warning",
  sourcemap: false,
  minify: false,
});

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
