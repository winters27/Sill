/**
 * Reaches the network without requiring a module.
 *
 * `fetch` is a global in modern Node, so the module gate never sees it. This
 * is the extension that would have walked straight through it.
 *
 * Reports what happened rather than crashing, so the same fixture can be run
 * with the permission and without it. The address is a closed local port, so
 * the granted case fails to connect rather than reaching anything real.
 */
const React = require("react");
const { List } = require("@raycast/api");

let outcome = "unknown";

const done = fetch("http://127.0.0.1:1/")
  .then(() => {
    outcome = "reachable";
  })
  .catch((err) => {
    outcome = /not allowed to open network connections/.test(String(err && err.message))
      ? "gated"
      : "reachable";
  });

module.exports.default = function Command() {
  const [said, setSaid] = React.useState("waiting");
  React.useEffect(() => {
    void done.then(() => setSaid(outcome));
  }, []);

  return React.createElement(List, null, React.createElement(List.Item, { key: "a", title: said }));
};
