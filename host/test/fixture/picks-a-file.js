/**
 * A form that asks somebody to choose a file, which is the field that was
 * declared by the API layer and drawn by nothing.
 *
 * Both shapes an extension uses: one picker for a single file and one that
 * takes several and directories as well. A form fixture rather than a real
 * extension because the store's file pickers sit behind a preference or a
 * platform Sill is not, and what has to be true here is that the field reaches
 * the window at all.
 *
 * The other half of the claim is not testable from here and is not left to
 * chance either. `scripts/verify-source.mjs` holds every `Form.*` component
 * the host declares to an arm in `FormView.svelte`, so a field that arrives is
 * a field that is drawn, and the chain cannot quietly grow a hole again.
 */
const React = require("react");
const { Form, ActionPanel, Action } = require("@raycast/api");

module.exports.default = function Command() {
  return React.createElement(
    Form,
    {
      actions: React.createElement(
        ActionPanel,
        null,
        React.createElement(Action.SubmitForm, {
          title: "Attach",
          onSubmit: (values) => {
            console.log(`attached ${JSON.stringify(values)}`);
          },
        }),
      ),
    },
    React.createElement(Form.TextField, { id: "note", title: "Note" }),
    React.createElement(Form.FilePicker, {
      id: "one",
      title: "Attachment",
      allowMultipleSelection: false,
    }),
    React.createElement(Form.FilePicker, {
      id: "many",
      title: "Sources",
      canChooseDirectories: true,
    }),
  );
};
