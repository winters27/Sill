/**
 * An extension built the way most of the Raycast store is built.
 *
 * `usePromise` for the data, `useCachedState` for something remembered, and a
 * helper from the same package. Before `@raycast/utils` existed here this
 * would not load at all, which is what shut a large slice of the catalogue
 * out.
 */
const React = require("react");
const { List } = require("@raycast/api");
const { usePromise, useCachedState, getAvatarIcon, withCache } = require("@raycast/utils");

const fetchFruit = withCache(async () => ["Apple", "Pear"]);

module.exports.default = function Command() {
  const { isLoading, data } = usePromise(fetchFruit, []);
  const [seen] = useCachedState("seen", "never");

  const avatar = getAvatarIcon("Ada Lovelace");

  return React.createElement(
    List,
    { isLoading },
    ...(data ?? []).map((name) =>
      React.createElement(List.Item, {
        key: name,
        title: name,
        subtitle: `${seen} ${avatar.source.startsWith("data:image/svg+xml") ? "avatar-ok" : "avatar-bad"}`,
      }),
    ),
  );
};
