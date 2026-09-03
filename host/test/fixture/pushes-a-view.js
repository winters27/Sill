/**
 * A list whose row opens a second screen, which is the shape of most of the
 * store: a list of things, and a detail of one thing.
 *
 * The mount counter is the point of the fixture rather than decoration. The
 * pushed screen writes a line to the console the first time it runs, so a
 * target that is mounted eagerly with the list, once per row, is visible in
 * the log instead of being a cost nobody can see. Two rows, so eager mounting
 * would say it twice before anything is pressed.
 */
const React = require("react");
const { List, Detail, ActionPanel, Action, useNavigation } = require("@raycast/api");

function Second({ name }) {
  React.useEffect(() => {
    console.log(`second screen mounted: ${name}`);
  }, [name]);

  const { pop } = useNavigation();

  return React.createElement(Detail, {
    markdown: `# ${name}\n\nPushed.`,
    actions: React.createElement(
      ActionPanel,
      null,
      React.createElement(Action, { title: "Go Back", onAction: pop }),
    ),
  });
}

function row(name) {
  return React.createElement(List.Item, {
    key: name,
    title: name,
    actions: React.createElement(
      ActionPanel,
      null,
      React.createElement(Action.Push, {
        title: "Show Details",
        target: React.createElement(Second, { name }),
      }),
    ),
  });
}

module.exports.default = function Command() {
  return React.createElement(List, null, row("First"), row("Second"));
};
