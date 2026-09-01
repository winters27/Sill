/**
 * An extension that never yields.
 *
 * Stands in for the loop somebody ships by accident: no crash, no completion,
 * just a thread pinned for as long as the launcher runs. Nothing else in the
 * host notices this, which is what the runaway budget is for.
 */
const React = require("react");
const { List } = require("@raycast/api");

module.exports.default = function Command() {
  // Deliberately without an exit. `worker.terminate()` is what ends it.
  for (;;) {
    Math.sqrt(Math.random());
  }

  // eslint-disable-next-line no-unreachable
  return React.createElement(List, null);
};
