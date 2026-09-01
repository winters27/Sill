/**
 * Reaches for something `@raycast/utils` does not have here.
 *
 * Called while the module loads rather than during a render, because React
 * absorbs a throw inside a component and turns it into its own error boundary
 * behaviour. What is being checked is the message, not where React puts it.
 */
const React = require("react");
const { List } = require("@raycast/api");
const { runAppleScript } = require("@raycast/utils");

runAppleScript();

module.exports.default = function Command() {
  return React.createElement(List, null);
};
