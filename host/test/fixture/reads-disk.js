/**
 * An extension that wants the filesystem.
 *
 * Requires `fs` at the top level on purpose, so a refusal happens while the
 * module is loading and arrives as an extension crash carrying the reason.
 * That is where a real extension would hit it too: bundlers hoist requires,
 * so the first thing a denied extension does is fail to start.
 */
const React = require("react");
const { List } = require("@raycast/api");
const fs = require("fs");

module.exports.default = function Command() {
  return React.createElement(
    List,
    null,
    React.createElement(List.Item, {
      key: "a",
      title: typeof fs.readFileSync === "function" ? "reached the disk" : "no disk",
    }),
  );
};
