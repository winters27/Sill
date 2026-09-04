/**
 * An extension built the way most of the Raycast store is built.
 *
 * `usePromise` for the data, `useCachedState` for something remembered, and a
 * helper from the same package. Before `@raycast/utils` existed here this
 * would not load at all, which is what shut a large slice of the catalogue
 * out.
 */
const React = require("react");
const { Cache, List } = require("@raycast/api");
const { usePromise, useCachedState, getAvatarIcon, withCache } = require("@raycast/utils");

const fetchFruit = withCache(async () => ["Apple", "Pear"]);

/*
 * The Cache, subscribed to the way the published `@raycast/utils` subscribes.
 *
 * It writes `useSyncExternalStore(cache.subscribe, ...)`, which hands React the
 * function without the object it came from. React calls it with no `this`, and
 * for as long as `subscribe` was an ordinary method that read
 * `this.namespace` of undefined and killed the command before its first
 * render. It cost a top-downloaded extension, and the error came out of React
 * with nothing in the stack naming Sill.
 *
 * Detached into a variable on purpose. Calling `cache.subscribe(...)` here
 * would pass whatever the fix was meant to prove.
 */
const cache = new Cache({ namespace: "fruit" });
const subscribeDetached = cache.subscribe;

module.exports.default = function Command() {
  const { isLoading, data } = usePromise(fetchFruit, []);
  const [seen] = useCachedState("seen", "never");

  const watched = React.useSyncExternalStore(
    subscribeDetached,
    () => "detached-ok",
    () => "detached-ok",
  );

  const avatar = getAvatarIcon("Ada Lovelace");

  return React.createElement(
    List,
    { isLoading },
    ...(data ?? []).map((name) =>
      React.createElement(List.Item, {
        key: name,
        title: name,
        subtitle: `${seen} ${watched} ${avatar.source.startsWith("data:image/svg+xml") ? "avatar-ok" : "avatar-bad"}`,
      }),
    ),
  );
};
