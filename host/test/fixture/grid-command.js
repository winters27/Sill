/**
 * Exercises Grid the way real extensions use it: sections, a column count,
 * string content for glyph-style tiles, and an action on an item.
 */
const React = require("react");
const { Grid, ActionPanel, Action, showToast, Toast } = require("@raycast/api");

module.exports.default = function Command() {
  return React.createElement(
    Grid,
    { columns: 4, searchBarPlaceholder: "Search shapes" },
    React.createElement(
      Grid.Section,
      { key: "s1", title: "Round" },
      React.createElement(Grid.Item, {
        key: "circle",
        title: "Circle",
        subtitle: "round",
        content: "●",
        actions: React.createElement(
          ActionPanel,
          null,
          React.createElement(Action, {
            title: "Pick Circle",
            onAction: async () => {
              await showToast({ title: "Picked Circle", style: Toast.Style.Success });
            },
          }),
        ),
      }),
      React.createElement(Grid.Item, {
        key: "ring",
        title: "Ring",
        content: "○",
      }),
    ),
    React.createElement(
      Grid.Section,
      { key: "s2", title: "Angular" },
      React.createElement(Grid.Item, { key: "square", title: "Square", content: "■" }),
      React.createElement(Grid.Item, { key: "triangle", title: "Triangle", content: "▲" }),
    ),
  );
};
