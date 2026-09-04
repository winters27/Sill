/**
 * Writes its own code at runtime, which is the thing that used to be listed as
 * a hole in the gate.
 *
 * It is not one, and this fixture is how that claim is kept honest rather than
 * asserted. `eval`, `new Function` and `WebAssembly` all make code; none of
 * them makes a capability. What they can reach is exactly what is already in
 * scope, and every route out of that scope is gated:
 *
 * - `require` is a parameter of the module wrapper rather than a global, so
 *   neither `new Function` nor an indirect `eval` can see it at all;
 * - a direct `eval` does see it, and it is the gated one;
 * - `process.getBuiltinModule` and `process.binding` are globals, and both are
 *   wrapped;
 * - `import()` written inside generated code meets the loader hook like any
 *   other;
 * - WebAssembly has no I/O of its own. Its only reach is a function JavaScript
 *   hands it, so the module below is given `process.getBuiltinModule` as an
 *   import and calling it is refused exactly the way calling it directly is.
 *
 * What generated code *does* defeat is the store's scan, which reads source
 * text and cannot see a module name assembled at runtime. That is a limit on
 * the description rather than a hole in the gate, and the two are different
 * claims.
 */
const React = require("react");
const { List } = require("@raycast/api");

/*
 * A WebAssembly module with one import and one export.
 *
 * `run` calls the imported function and returns nothing. Written out as bytes
 * because a fixture that needs a toolchain to build is a fixture nobody runs:
 * header, one `() -> ()` type, one import `e.f`, one function, one export
 * named `run`, and a body that is `call 0; end`.
 */
const WASM = new Uint8Array([
  0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00,
  0x01, 0x04, 0x01, 0x60, 0x00, 0x00,
  0x02, 0x07, 0x01, 0x01, 0x65, 0x01, 0x66, 0x00, 0x00,
  0x03, 0x02, 0x01, 0x00,
  0x07, 0x07, 0x01, 0x03, 0x72, 0x75, 0x6e, 0x00, 0x01,
  0x0a, 0x06, 0x01, 0x04, 0x00, 0x10, 0x00, 0x0b,
]);

/** Whether a thrown thing is Sill refusing, rather than anything else. */
const refused = (err) => /^sill:/.test(String(err && err.message));

const reached = [];
const notes = [];

function record(name, attempt) {
  try {
    const got = attempt();
    const usable = got && typeof got.readFileSync === "function";
    if (usable) reached.push(name);
    notes.push(`${name}: ${usable ? "REACHED" : "nothing"}`);
  } catch (err) {
    if (!refused(err)) {
      // Not a refusal and not a success: the escape did not even run, which
      // for `require` out of a generated scope is the whole point.
      notes.push(`${name}: unavailable (${String(err && err.message).split("\n")[0]})`);
      return;
    }
    notes.push(`${name}: refused`);
  }
}

record("function-require", () => new Function("return typeof require === 'function' ? require('fs') : null")());
record("indirect-eval-require", () => (0, eval)("typeof require === 'function' ? require('fs') : null"));
record("direct-eval-require", () => eval("require('fs')"));
record("function-getBuiltinModule", () => new Function("return process.getBuiltinModule('fs')")());
record("function-binding", () => new Function("return process.binding('fs')")());

record("wasm-import", () => {
  const instance = new WebAssembly.Instance(new WebAssembly.Module(WASM), {
    e: { f: () => process.getBuiltinModule("fs") },
  });
  instance.exports.run();
  return null;
});

if (reached.length > 0) {
  throw new Error(`sill-test: generated code reached the disk through ${reached.join(", ")}`);
}

throw new Error(`sill-test: generated code reached nothing. ${notes.join(" | ")}`);

// eslint-disable-next-line no-unreachable
module.exports.default = function Command() {
  return React.createElement(List, null);
};
