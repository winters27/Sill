# The extension host

This is the Node process that runs Raycast-compatible extensions for Sill.
Rust starts it, talks to it over stdio, and the window draws whatever it says
to draw. Nothing in here knows about Tauri, and nothing in Rust knows about
React.

If you are writing an extension rather than working on this, you want
[docs/extensions.md](../docs/extensions.md) instead.

## What it is for

An extension is somebody else's code. It should not be able to stop the
launcher, exhaust the machine, or reach anything nobody agreed to it
reaching, and it should not have to be reloaded for any of that to be true.
The shape of this process follows from that.

- **One worker thread per running command.** A command that hangs, leaks or
  crashes takes its own thread with it.
- **The window never receives a whole view.** React already knows exactly
  what changed, so what crosses is a stream of small operations against a
  tree the window keeps its own copy of.
- **Permissions are read per call, not per launch.** Revoking one in Settings
  reaches a command that is running right now.

## Layout

| Path | What is there |
| --- | --- |
| `src/index.ts` | The entry point, and the worker body. One file, selected by `isMainThread` |
| `src/proto/` | The stdio wire: length-prefixed frames, and JSON-RPC over them |
| `src/api/` | The module an extension gets for `@raycast/api` |
| `src/utils/` | The module it gets for `@raycast/utils` |
| `src/render/` | The React reconciler, the node tree, and the handler registry |
| `src/worker/` | Worker startup, the module gate and the network gate |
| `test/` | Smoke, integration and runaway tests, plus the fixtures they drive |

## The wire

Every frame is a 4-byte big-endian length followed by that many bytes of
UTF-8 JSON. The length excludes the header. Rust reads these with a
length-delimited codec configured the same way, so a change here has to be
made there in the same commit.

Above the framing is JSON-RPC, in both directions. Rust asks the host to
start a command, activate a handler or change what an extension holds; the
host asks Rust for the clipboard, a file, an application list, a dialog.

## How a view reaches the screen

An extension renders with React into a reconciler that has no DOM behind it.
Each host instance gets an id that never moves, and the reconciler's own
mutations become the operation stream: create this node, set these props, put
it there, take it away.

Two things about that are load-bearing.

- **A function cannot cross the wire.** Every `onAction`, `onChange` and
  `onSearchTextChange` is swapped for an opaque id, and the window sends the
  id back when somebody presses the row. Removing an id is deferred by one
  commit, because React routinely detaches a handler and immediately attaches
  an equivalent one, and an activation already in flight would otherwise land
  on a dead button.
- **An element passed as a prop is not a prop.** Raycast writes
  `actions={<ActionPanel/>}` and `detail={<List.Item.Detail/>}`. React
  elements are circular, so serialising one throws, and the reconciler never
  descends into props, so a handler inside one would never be registered.
  Those are lifted into the tree as a `$slot` child carrying the prop's name,
  which puts the whole thing back under React where diffing and cleanup work
  the way they do everywhere else.

## Where the limits are

- **Heap.** Each worker is capped, so one extension cannot take the machine.
- **Runaway.** Event loop utilisation is watched rather than processor time.
  A real extension is almost entirely idle, waking to render and to answer;
  a thread that never yields is a loop, and it is stopped.
- **Shutdown.** A worker gets a grace period to unmount cleanly and is
  terminated if it does not take it.
- **Output.** An extension's `console` writes are read off both worker
  streams and forwarded, bounded per command and per line. They used to be
  discarded, which made them invisible and, because nothing drained the
  stream, an unbounded buffer as well.

## Building and testing

```bash
npm --prefix host run build
npm --prefix host test
```

The build is one bundled CommonJS file, and it has to stay one file: the
worker is spawned from the same artifact. React and the reconciler are
bundled in rather than left external, because one React instance per worker
is not negotiable and a second copy resolved from somewhere else breaks hooks
in ways that are miserable to diagnose.

`npm --prefix host test` runs the type check, a smoke render, the integration
tests and the runaway test. From the repository root, `npm run gate:views`
goes further and renders real extensions end to end; see
[docs/extensions.md](../docs/extensions.md) for how to fetch those.
