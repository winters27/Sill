/**
 * A list whose callback is inside a prop rather than beside it.
 *
 * `pagination` is an object carrying two numbers and a function, which is the
 * one shape in the API where a callback is not at the top of the prop bag.
 * Serialising only the top level left the function to be dropped by JSON, so
 * the window drew a list that claimed there was more and named nobody to ask.
 */
const React = require("react");
const { List } = require("@raycast/api");

module.exports.default = function Command() {
  return React.createElement(
    List,
    {
      pagination: {
        pageSize: 20,
        hasMore: true,
        onLoadMore: () => console.log("asked for more"),
      },
    },
    React.createElement(List.Item, { key: "a", title: "Row 1" }),
  );
};
