/**
 * Proves the reconciler turns a React tree into an op stream, that a re-render
 * emits a patch rather than a whole tree, and that handler ids survive
 * re-rendering so the UI can activate them.
 *
 * Run: node scripts/run-smoke.mjs
 */
import React, { useState } from "react";
import { createRenderer } from "../src/render/renderer";
import type { Op } from "../src/render/nodes";

const commits: Op[][] = [];
const renderer = createRenderer({
  onCommit: (ops) => commits.push(ops),
});

let fired: unknown[] = [];

function List({ title, count }: { title: string; count: number }) {
  return React.createElement(
    "List",
    { searchBarPlaceholder: title },
    Array.from({ length: count }, (_, i) =>
      React.createElement("List.Item", {
        key: String(i),
        title: `item ${i}`,
        onAction: (...args: unknown[]) => {
          fired = ["item", i, ...args];
        },
      }),
    ),
  );
}

function assert(cond: boolean, msg: string) {
  if (!cond) {
    console.error(`FAIL: ${msg}`);
    process.exitCode = 1;
  } else {
    console.log(`ok   ${msg}`);
  }
}

// First render: three items.
renderer.render(React.createElement(List, { title: "Search", count: 3 }));

const first = commits[0] ?? [];
assert(commits.length === 1, "first render produced exactly one commit");
assert(
  first.filter((o) => o.op === "create").length === 4,
  `created 4 instances (1 List + 3 items), got ${first.filter((o) => o.op === "create").length}`,
);

const listCreate = first.find((o) => o.op === "create" && o.$t === "List");
assert(listCreate !== undefined, "List instance was created");

const itemCreate = first.find((o) => o.op === "create" && o.$t === "List.Item");
assert(
  itemCreate !== undefined &&
    typeof (itemCreate as { props: Record<string, unknown> }).props.onAction === "object",
  "function prop was replaced with a handler reference",
);

const handlerRef = (itemCreate as { props: Record<string, { $handler: string }> }).props.onAction;
assert(typeof handlerRef.$handler === "string", "handler reference carries an id");

// Second render: same shape, one prop changed. This is the efficiency claim.
renderer.render(React.createElement(List, { title: "Search files", count: 3 }));

const second = commits[1] ?? [];
assert(commits.length === 2, "second render produced a second commit");
assert(
  second.length < first.length,
  `re-render emitted a patch (${second.length} ops) smaller than the initial build (${first.length} ops)`,
);
assert(
  second.every((o) => o.op === "updateProps"),
  `re-render emitted only prop updates, got ${JSON.stringify(second.map((o) => o.op))}`,
);

// The handler id must be stable across the re-render, or the UI's pending
// activation would hit a dead id.
assert(renderer.callbacks.has(handlerRef.$handler), "handler id survived the re-render");

renderer.callbacks.invoke(handlerRef.$handler, ["arg"]);
assert(
  Array.isArray(fired) && fired[0] === "item" && fired[2] === "arg",
  `invoking by id ran the extension's callback, got ${JSON.stringify(fired)}`,
);

// Third render: fewer items, so removals must appear.
renderer.render(React.createElement(List, { title: "Search files", count: 1 }));
const third = commits[2] ?? [];
assert(
  third.some((o) => o.op === "remove"),
  `shrinking the list emitted remove ops, got ${JSON.stringify(third.map((o) => o.op))}`,
);

console.log(`\ncommits: ${commits.map((c) => c.length).join(", ")} ops`);
console.log(JSON.stringify(commits[1], null, 2));
