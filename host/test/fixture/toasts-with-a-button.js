/**
 * A toast with a button on it, which is what a command does when it fails.
 *
 * The shape is the ecosystem's: something did not work, the toast says so, and
 * it offers to try again. Raycast hands the handler the toast itself, so the
 * button's own code rewrites the message it is sitting on, and that is what
 * makes this testable without a window: the second toast can only exist if the
 * button ran, and it can only carry the right words if the handle it was given
 * was the live one rather than a copy.
 *
 * A `List` underneath, because a toast is not a view. A command showing one is
 * still drawing something, and a fixture that rendered nothing would be
 * testing the no-view path by accident.
 */
const React = require("react");
const { List, showToast, Toast } = require("@raycast/api");

module.exports.default = function Command() {
  React.useEffect(() => {
    void (async () => {
      const toast = await showToast({
        style: Toast.Style.Failure,
        title: "Could not reach the server",
        message: "It answered nothing",
        primaryAction: {
          title: "Try Again",
          shortcut: { modifiers: ["cmd"], key: "r" },
          onAction: (shown) => {
            console.log("the button ran");
            shown.style = Toast.Style.Animated;
            shown.title = "Trying again";
          },
        },
        secondaryAction: {
          title: "Give Up",
          onAction: (shown) => {
            void shown.hide();
          },
        },
      });

      // Nothing else touches it. A test pressing the button is the only thing
      // that changes what is on screen after this.
      void toast;
    })();
  }, []);

  return React.createElement(
    List,
    null,
    React.createElement(List.Item, { key: "one", title: "Nothing loaded" }),
  );
};
