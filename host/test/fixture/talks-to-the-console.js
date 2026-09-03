/**
 * An extension that says things while it renders.
 *
 * `console.log` is the first thing anybody writing one of these reaches for,
 * and until the host read the worker's streams it was the one thing that went
 * nowhere at all: not to a terminal, not to Sill's log, not anywhere the
 * person who wrote it could look.
 */
const React = require("react");
const { List } = require("@raycast/api");

module.exports.default = function Command() {
  console.log("the fruit list is being drawn");
  console.error("and something about it went wrong");

  // Longer than one line is worth carrying whole. An extension logging a
  // document it fetched writes one of these without meaning to.
  console.log(`start-of-a-long-line${"x".repeat(4000)}end-of-a-long-line`);

  return React.createElement(List, null);
};
