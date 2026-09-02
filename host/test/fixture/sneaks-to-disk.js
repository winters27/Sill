/**
 * An extension that goes round the front door.
 *
 * Three ways to a builtin that never touch `Module.prototype.require`, which
 * is the only thing the gate used to guard:
 *
 * - `Module._load`, which `require` is a thin wrapper over.
 * - `module.createRequire`, which hands back a fresh `require` bound to a path
 *   the caller picks.
 * - `process.getBuiltinModule`, documented and present since Node 22.3, which
 *   returns the builtin directly and was by far the shortest way out.
 *
 * Each is tried in turn and the first one that yields `fs` wins, so this
 * fixture reports which escape worked rather than only that one did. Loading
 * it at the top level on purpose: a refusal then arrives as a crash carrying
 * the reason, which is where a real extension would meet it.
 */
const React = require("react");
const { List } = require("@raycast/api");

const attempts = [
  ["Module._load", () => require("module")._load("fs", module, false)],
  ["createRequire", () => require("module").createRequire(__filename)("fs")],
  ["getBuiltinModule", () => process.getBuiltinModule("fs")],
];

let got = null;
let how = "nothing worked";
const refusals = [];

for (const [name, attempt] of attempts) {
  try {
    const fs = attempt();
    if (fs && typeof fs.readFileSync === "function") {
      got = fs;
      how = name;
      break;
    }
  } catch (err) {
    refusals.push(`${name}: ${err.message}`);
  }
}

// Thrown rather than rendered, so the test sees a crash with the reason in it
// exactly as it does for the ordinary `require` path.
if (got) {
  throw new Error(`sill-test: reached the disk through ${how}`);
}

throw new Error(`sill-test: every escape was refused. ${refusals.join(" | ")}`);

module.exports.default = function Command() {
  return React.createElement(List, null);
};
