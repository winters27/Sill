/**
 * Goes under the module system rather than round it.
 *
 * Every attempt here reaches something a module gate never sees, and all of
 * them worked against the gate as it stood:
 *
 * - `process.binding("fs")` is the C++ binding `fs` is written on top of. It
 *   is deprecated, it is still there in Node 25, and it reads files.
 * - `process._linkedBinding` is its neighbour.
 * - `process.dlopen` loads a native addon, which is machine code running
 *   outside every permission Sill has.
 * - `node:v8` writes a heap snapshot to any path it is handed, `node:sqlite`
 *   opens database files and `node:vm` builds a loader of its own. None was
 *   named in a table of dangerous modules, which is what is wrong with a table
 *   of dangerous modules.
 * - `Module.registerHooks` would put an extension's own resolver in front of
 *   the one enforcing its permissions.
 *
 * Reports which ones got through, so a fixture whose escapes were simply
 * broken cannot pass by accident. Thrown at module load, where a real
 * extension would meet it.
 */
const React = require("react");
const { List } = require("@raycast/api");
// Both free, and deliberately so: neither reaches anything outside the worker.
const { tmpdir } = require("node:os");
const { join } = require("node:path");

const attempts = [
  ["binding", () => process.binding("fs")],
  ["linkedBinding", () => process._linkedBinding("fs")],
  // A path that is not there on purpose. If the stub ever goes, this reaches
  // the real `dlopen` and fails as Node rather than as Sill, which is exactly
  // what the test looks at, and nothing is actually loaded into the process.
  ["dlopen", () => process.dlopen({ exports: {} }, "C:\\sill-no-such-addon.node")],
  // Signal zero asks whether a process is there rather than ending it, so the
  // fixture can try the call without being the thing that breaks the run.
  ["kill", () => process.kill(process.pid, 0)],
  // A diagnostic report is a file, written wherever it is pointed.
  ["report", () => process.report.writeReport(join(tmpdir(), "sill-fixture-report.json"))],
  ["v8", () => require("node:v8")],
  ["sqlite", () => require("node:sqlite")],
  ["vm", () => require("node:vm")],
  ["registerHooks", () => require("node:module").registerHooks({ resolve: (s, c, n) => n(s, c) })],
];

const got = [];
const refusals = [];

for (const [name, attempt] of attempts) {
  try {
    attempt();
    got.push(name);
  } catch (err) {
    refusals.push(`${name}: ${err.message}`);
  }
}

if (got.length > 0) {
  throw new Error(`sill-test: got under the gate through ${got.join(", ")}`);
}

throw new Error(`sill-test: every way under was refused. ${refusals.join(" | ")}`);

// eslint-disable-next-line no-unreachable
module.exports.default = function Command() {
  return React.createElement(List, null);
};
