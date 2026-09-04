import { defineConfig } from "vitest/config";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import { resolve } from "node:path";

const lib = { $lib: resolve(import.meta.dirname, "src/lib") };

/**
 * Two suites, because they want opposite things.
 *
 * **Logic** is the one that already existed and its note still holds: these
 * exercise plain modules, and loading the whole framework to run arithmetic
 * would make the suite slow enough that nobody runs it. It stays on `node`
 * with no plugins, and it is what almost every test here is.
 *
 * **Components** is the gap `P7-03` names. Nothing could render a component
 * before this, so anything a component decides for itself was verified by
 * looking at it in a browser, which is not something a build can do. The
 * recurring bugs in this codebase are exactly that shape: a duplicate key in
 * a keyed `{#each}` blanks the whole list rather than drawing twice, and a
 * row that draws but never changes is keyed by the wrong identity.
 *
 * Kept apart rather than switched on for everything, so the cost lands only
 * on the files that need it. A component test names itself `*.svelte.test.ts`
 * and gets a DOM; everything else is untouched and still runs in a second.
 */
export default defineConfig({
  test: {
    projects: [
      {
        resolve: { alias: lib },
        test: {
          name: "logic",
          include: ["src/**/*.test.ts"],
          exclude: ["src/**/*.svelte.test.ts"],
          environment: "node",
        },
      },
      {
        plugins: [svelte({ hot: false })],
        resolve: {
          alias: lib,
          // The browser build, so lifecycle and effects run as they do in the
          // window. Without this the server build is resolved and a component
          // renders once to a string, which cannot answer any of the
          // questions these tests are for.
          conditions: ["browser"],
        },
        test: {
          name: "components",
          include: ["src/**/*.svelte.test.ts"],
          environment: "happy-dom",
        },
      },
    ],
  },
});
