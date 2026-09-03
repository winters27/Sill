/**
 * A list wearing every part of a row that is drawn rather than run.
 *
 * A fixture rather than a real extension because the real ones each use two or
 * three of these and no single one uses all of them, so holding the set
 * together means holding it here. Every shape below is one a store extension
 * actually passes: a bare `Icon.` name, a `{ source, tintColor }`, an emoji,
 * a text accessory, a tag accessory with a colour, and a detail pane with
 * every kind of metadata row in it.
 */
const React = require("react");
const { List, ActionPanel, Action, Icon, Color } = require("@raycast/api");

function detail(name) {
  return React.createElement(List.Item.Detail, {
    markdown: `# ${name}\n\nA **detail** pane with a [link](https://example.invalid).\n\n- one\n- two`,
    metadata: React.createElement(
      List.Item.Detail.Metadata,
      null,
      React.createElement(List.Item.Detail.Metadata.Label, {
        key: "kind",
        title: "Kind",
        text: name,
        icon: Icon.Document,
      }),
      React.createElement(List.Item.Detail.Metadata.Separator, { key: "rule" }),
      React.createElement(List.Item.Detail.Metadata.Link, {
        key: "home",
        title: "Home",
        text: "example.invalid",
        target: "https://example.invalid",
      }),
      React.createElement(
        List.Item.Detail.Metadata.TagList,
        { key: "tags", title: "Tags" },
        React.createElement(List.Item.Detail.Metadata.TagList.Item, {
          key: "a",
          text: "stable",
          color: Color.Green,
        }),
        React.createElement(List.Item.Detail.Metadata.TagList.Item, {
          key: "b",
          text: "slow",
          color: Color.Yellow,
        }),
      ),
    ),
  });
}

module.exports.default = function Command() {
  return React.createElement(
    List,
    {
      isShowingDetail: true,
      searchBarAccessory: React.createElement(
        List.Dropdown,
        { tooltip: "Which set", onChange: () => {} },
        React.createElement(
          List.Dropdown.Section,
          { key: "s", title: "Sets" },
          React.createElement(List.Dropdown.Item, { key: "all", title: "All", value: "all" }),
          React.createElement(List.Dropdown.Item, { key: "mine", title: "Mine", value: "mine" }),
        ),
      ),
      actions: React.createElement(
        ActionPanel,
        null,
        React.createElement(Action, { title: "Nothing", onAction: () => {} }),
      ),
    },
    React.createElement(List.EmptyView, {
      key: "empty",
      title: "Nothing here yet",
      description: "This fixture would say so in its own words.",
    }),
    React.createElement(List.Item, {
      key: "named",
      title: "A named icon",
      subtitle: "Icon.Star",
      icon: Icon.Star,
      accessories: [{ text: "12 items" }, { tag: { value: "ready", color: Color.Green } }],
      detail: detail("A named icon"),
      actions: React.createElement(
        ActionPanel,
        null,
        React.createElement(Action, { title: "Do It", onAction: () => {} }),
      ),
    }),
    React.createElement(List.Item, {
      key: "tinted",
      title: "A tinted icon",
      icon: { source: Icon.CheckCircle, tintColor: Color.Green },
      accessories: [{ text: "one", tooltip: "the first one" }],
      detail: detail("A tinted icon"),
    }),
    React.createElement(List.Item, {
      key: "emoji",
      title: "An emoji icon",
      icon: "🎉",
      keywords: ["party"],
      accessories: [{ tag: "beta" }],
      detail: detail("An emoji icon"),
    }),
  );
};
