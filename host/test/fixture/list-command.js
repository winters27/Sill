/**
 * Stands in for an extension bundle as `ray build` would emit it: CommonJS,
 * a default export, importing only "@raycast/api" and "react".
 */
const React = require("react");
const { List, ActionPanel, Action, showToast, Toast } = require("@raycast/api");

module.exports.default = function Command() {
  return React.createElement(
    List,
    { searchBarPlaceholder: "Search fruit" },
    React.createElement(List.Item, {
      key: "a",
      title: "Apple",
      subtitle: "a pome",
      actions: React.createElement(
        ActionPanel,
        null,
        React.createElement(Action, {
          title: "Pick it",
          onAction: async () => {
            await showToast({ title: "Picked Apple", style: Toast.Style.Success });
          },
        }),
      ),
    }),
    React.createElement(List.Item, { key: "b", title: "Banana", subtitle: "a berry" }),
  );
};
