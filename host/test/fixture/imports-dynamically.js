/**
 * Reaches a built-in through the other loader.
 *
 * `await import("node:fs")` never touches `Module._load`, so every gate on
 * that side was beside the point for one line of ordinary code. This is the
 * escape the last pass wrote down and left open, and the fixture that says
 * whether the loader hook closed it.
 *
 * Three ways in, because the hook has to catch all of them:
 *
 * - the plain specifier, and the same one without the `node:` prefix;
 * - a `data:` module, whose own `import` has to be caught too, since an
 *   extension can write the module that does the reaching rather than doing it
 *   itself.
 *
 * Reports rather than crashes, so the same file runs with the permission and
 * without it, which is the only pair that proves anything.
 */
const React = require("react");
const { List } = require("@raycast/api");

const gated = (err) =>
  /not allowed to read and change files/.test(String(err && err.message)) ? "refused" : "other";

async function attempt(name, load) {
  try {
    const fs = await load();
    const real = fs && (fs.readFileSync || (fs.default && fs.default.readFileSync));
    return `${name}-${typeof real === "function" ? "reached" : "empty"}`;
  } catch (err) {
    return `${name}-${gated(err)}`;
  }
}

const done = Promise.all([
  attempt("prefixed", () => import("node:fs")),
  attempt("bare", () => import("fs")),
  // The module an extension writes itself. Its `import` is a second hop and
  // has to meet the same hook, or writing the escape as a string is the escape.
  attempt("dataurl", () => import("data:text/javascript,export * from 'node:fs';")),
]);

module.exports.default = function Command() {
  const [said, setSaid] = React.useState("waiting");
  React.useEffect(() => {
    void done.then((all) => setSaid(all.join(" ")));
  }, []);

  return React.createElement(List, null, React.createElement(List.Item, { key: "a", title: said }));
};
