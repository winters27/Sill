import { defineConfig } from "vitest/config";
import { resolve } from "node:path";

/**
 * Tests for the frontend's own logic.
 *
 * Deliberately not the SvelteKit config: these exercise plain modules, and
 * loading the whole framework to run arithmetic would make the suite slow
 * enough that nobody runs it. `$lib` is resolved by hand for the same reason.
 */
export default defineConfig({
  resolve: {
    alias: { $lib: resolve(import.meta.dirname, "src/lib") },
  },
  test: {
    include: ["src/**/*.test.ts"],
    environment: "node",
  },
});
