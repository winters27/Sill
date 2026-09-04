/**
 * An extension that uses its permissions again, later, on demand.
 *
 * The fixture for revoking. `reads-disk.js` asks once while its module loads,
 * which only ever proves what a **new** worker is told; this one keeps running
 * and tries again whenever the launcher activates its action, so the same
 * worker can be asked before a revoke and after one.
 *
 * Both gates in one command on purpose. `require` and `fetch` are refused by
 * different code in `patch-require.ts`, and only one of them used to be wrapped
 * at all when the permission was held, so a test that checked one would have
 * passed while the other went on working.
 *
 * Reports rather than crashes, because the whole point is being asked twice.
 */
const React = require("react");
const { List, ActionPanel, Action } = require("@raycast/api");

/**
 * What `require("node:fs")` does right now.
 *
 * The refusal is reported with its own sentence attached, because a test that
 * only sees "refused" cannot tell a permission gate from a broken fixture, and
 * the sentence is what a person is supposed to be able to act on.
 */
function disk() {
  try {
    const fs = require("node:fs");
    return typeof fs.readFileSync === "function" ? "disk-reached" : "disk-empty";
  } catch (err) {
    return `disk-refused[${err && err.message}]`;
  }
}

/** And the same question of the network, which no require ever mentions. */
async function net() {
  try {
    // A closed local port: granted, this fails to connect rather than reaching
    // anything real, and the connection error is not the refusal.
    await fetch("http://127.0.0.1:1/");
    return "net-reached";
  } catch (err) {
    const said = String(err && err.message);
    return /not allowed to open network connections/.test(said)
      ? `net-refused[${said}]`
      : "net-reached";
  }
}

module.exports.default = function Command() {
  const [said, setSaid] = React.useState("waiting");

  const tryAgain = React.useCallback(() => {
    const now = disk();
    void net().then((over) => setSaid(`${now} ${over}`));
  }, []);

  React.useEffect(tryAgain, [tryAgain]);

  return React.createElement(
    List,
    null,
    React.createElement(List.Item, {
      key: "a",
      title: said,
      actions: React.createElement(
        ActionPanel,
        null,
        React.createElement(Action, { title: "Try Again", onAction: tryAgain }),
      ),
    }),
  );
};
