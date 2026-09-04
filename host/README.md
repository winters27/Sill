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
| `test/` | Smoke, integration, runaway and resource tests, plus the fixtures they drive |

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

- **Heap.** Each worker is capped at 512 MB, so one extension cannot take the
  machine. It is a backstop and not a budget: crossing it is V8 refusing to
  grow the heap and the thread ending where it stands, so whatever the person
  was doing in that command is lost. That is why it sits eight times above the
  heaviest real extension measured (Emoji Search, 63 MB) rather than anywhere
  near it, and why nothing warns at a lower line. When it does fire, what
  reaches the window is a sentence naming the command and the limit rather
  than a stack trace, and the host writes the same thing to its own stderr so
  a `no-view` command dying of memory leaves a mark somewhere.
- **Runaway.** Event loop utilisation is watched rather than processor time.
  A real extension is almost entirely idle, waking to render and to answer;
  a thread that never yields is a loop, and it is stopped.
- **Shutdown.** A worker gets a grace period to unmount cleanly and is
  terminated if it does not take it.
- **Output.** An extension's `console` writes are read off both worker
  streams and forwarded, bounded per command and per line. They used to be
  discarded, which made them invisible and, because nothing drained the
  stream, an unbounded buffer as well.
- **Diagnostics.** `Manager/diagnostics` answers with what every loaded
  command is holding and its share of a processor core. **Asked, never
  watched**: sampling on a timer would be a wakeup on a machine where nothing
  is happening, which is the one thing this project refuses to spend, so
  somebody opening the Extensions panel is the only reason to look. The memory
  figure comes from inside the worker over the control channel, because there
  is no way to read one worker's heap from the thread that made it; it is not
  a stream, deliberately, since both of a worker's streams are already
  diverted and drained by hand. A worker stuck in a loop cannot answer, which
  is reported as not answering rather than waited out, and the share of a core
  beside it is measured from outside and arrives regardless. `Manager/unload`
  takes the same reading on the way out, which is the last moment it exists.

## Where the numbers come from

`scripts/run-extension.mjs --measure` opens a command twice against a real
host, cold and then warm, and asks for a reading. The figures for the five
real extensions the view gate draws are in
[docs/budgets.md](../docs/budgets.md).

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
tests, the runaway test and the resource test. From the repository root,
`npm run gate:views` goes further and renders real extensions end to end; see
[docs/extensions.md](../docs/extensions.md) for how to fetch those.
