/**
 * An extension that keeps everything it allocates.
 *
 * Stands in for the leak somebody ships by accident: a cache with no eviction,
 * a list appended to on every render, a subscription that never unsubscribes.
 * Nothing crashes and nothing finishes; the heap simply goes up until V8 will
 * not grow it any further.
 *
 * Held in a module-level array on purpose. Allocating and dropping is what a
 * garbage collector is for and would prove nothing: what a heap cap is about
 * is memory that is still reachable.
 *
 * Many small objects rather than a few large buffers, and that is the whole
 * shape of the test. Both of the obvious ways to write this do not work:
 * `Uint8Array` keeps its bytes outside the JavaScript heap entirely, and a
 * megabyte string goes to large object space, so a worker can hold a gigabyte
 * of either and the cap never fires. What the cap governs is the ordinary
 * heap, which is where a few hundred thousand small objects go. Measured: this
 * shape reaches a 48 MB cap in about a second, and neither other shape reached
 * it in twenty.
 *
 * That is also the honest model of the fault. An extension does not leak by
 * allocating buffers; it leaks by keeping every row it has ever built.
 */
const React = require("react");
const { List } = require("@raycast/api");

const kept = [];

module.exports.default = function Command() {
  // On a timer rather than in a loop, so the worker stays able to answer while
  // it grows. An extension that pins its own event loop as well is a different
  // fault and the runaway budget already covers it; this one is only memory.
  const grow = setInterval(() => {
    for (let i = 0; i < 20_000; i++) {
      kept.push({ at: Math.random(), title: `row ${i}` });
    }
  }, 5);

  // Unreferenced deliberately: nothing here is meant to stop.
  void grow;

  return React.createElement(List, null);
};
