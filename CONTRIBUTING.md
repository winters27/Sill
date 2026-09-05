# Contributing to Sill

Thanks for looking. This page is about how Sill is built, not how to
build it. [The README](README.md) covers building, verifying, releasing
and the repository layout, and it is worth reading first.

Everything here comes down to one sentence:

> Rust is the brain. Svelte is the presenter. Tauri is the bridge.

Sill is meant to become very capable while staying very small at rest.
Those two goals fight each other, and almost every convention below
exists to keep the second one from losing.

## Before you write the feature

The most useful thing you can do is decide where each piece of work
belongs **before** implementing it. Write down the operations your
feature needs and give each one an owner:

| Operation | Owner | Why |
| --- | --- | --- |
| Read something from the OS | Rust | domain and OS work |
| Parse, normalise, validate | Rust | reusable, and not the UI's job |
| Match, rank, sort, score | Rust | latency-sensitive |
| Decide which actions apply | Rust | capability, not presentation |
| Cache anything | Rust | needs a lifetime and a bound |
| Draw a row | Svelte | presentation |
| Hover, focus, selection | Svelte | ephemeral interaction |
| Run the chosen action | Rust | capability |

If significant parsing, ranking, filtering, filesystem work or business
logic ends up on the Svelte side, that is worth a sentence in the pull
request explaining why. Sometimes there is a good reason. It should be
stated rather than assumed.

## The capability is not the UI

A feature should survive its own interface. Ask:

> If the Svelte surface disappeared tomorrow, would this feature still
> exist?

For anything substantial the answer should be yes, because Sill reaches
the same capabilities from several directions: launcher results, the
action panel, settings, the command line, AI tools, MCP and extensions.
Those should all arrive at one implementation.

If you find yourself writing an operation that another surface already
performs, use the existing one rather than adding a second.

## What belongs in Svelte

Genuinely quite a lot, and none of it domain logic:

- DOM structure, composition, styling
- focus, hover, selection, open and closed panels
- simple form and drag state
- animation and accessibility presentation
- keyboard interaction for the surface you are in
- measuring rendered DOM when there is no other way

Keep that state as narrow as it will go.

What does not belong there: fuzzy matching, ranking, search indexing,
expensive filtering or sorting, file parsing, metadata extraction,
caching policy, persistence rules, capability resolution, Windows
integration, filesystem traversal, or background polling.

## Send answers, not corpora

Every call across the Tauri boundary costs serialisation, allocation,
garbage collection and a reactive update. Design calls around user
intent.

Ask for the twelve best matches and let Rust return twelve. Do not fetch
the whole searchable set and discover them in JavaScript.

`execute_action(id, target)` is better than fetching metadata, deciding
the action type, building a shell command in JavaScript and running it.

That said, do not overcorrect: a keystroke, a hover or a cursor move is
not a round trip. Ephemeral visual interaction stays local.

## Tauri commands stay thin

A command should validate its input, call a service, translate the
result, and return. The feature lives underneath it.

```rust
#[tauri::command]
fn search(query: &str, services: State<'_, Services>) -> Vec<Hit> {
    services.search.query(query)
}
```

Not a command that loads a database, normalises a query, scans entries,
ranks them, updates frecency and builds actions. That is a service
wearing a command's clothes.

## Idle is a feature

Sill spends most of its life closed. The ideal idle workload is: nothing
changed, so nothing happened.

Before adding anything that recurs, check whether the OS can tell you
instead. Prefer an event over a timer. If polling really is unavoidable,
say why in a comment, pick the slowest cadence that works, stop while
the surface is hidden, and stop entirely when the feature is off.

A feature that is disabled should be close to free: no polling, no
connections, no loaded models, no spawned processes.

## Pay when used, not because you exist

Something used twice a week should not hold memory all week. Prefer:

```text
dormant → requested → initialise → active → release → dormant
```

over initialising everything at startup and keeping it alive until the
process exits.

If you add a subsystem, say how it stops. Startup code without teardown
code is unfinished.

## The search path is sacred

This sequence is latency-critical:

```text
keypress → query → matching → ranking → results → DOM → frame
```

Do not put unrelated work on it. Search work should avoid blocking file
and network operations, avoid locks shared with other subsystems,
minimise allocation and cloning, and return a bounded number of results.

A feature unrelated to search should not make search slower.

## Actions are capabilities, not click handlers

New operations belong in the action system, which decides what can run,
what it applies to, whether it needs confirmation, whether it can be
undone, and what policy governs it. A button is one way to invoke an
action, not the place to implement one.

## The design system is enforced

`src/lib/theme/theme.css` owns every size and colour. A literal pixel
font size or an inline accent colour in a component fails
`npm run verify:source`. The accent is for selection, matches, focus and
affirmative state, and nothing else.

`verify:source` checks a number of other things that no test can reach.
When it complains, it is usually right, and its message says what to do.

## Before you open a pull request

```bash
npm run verify
```

That runs the extension host tests, `svelte-check`, the web tests, the
Rust suite, the source-shape check, a tree check, and a gate that loads
real Raycast extensions. On a fresh clone, fetch those extensions first
with `npm run extensions:fetch`, or the gate has nothing to draw.

**Read the exit code, not the summary line.** The suite can print a
healthy-looking test count and still exit non-zero when something
underneath it failed or never ran.

If Sill is running, the Rust half cannot relink and the suite fails with
`failed to remove file ... sill.exe`. Close it first.

Tests are worth more when you have watched them fail. If you add one,
break the thing it covers and check that it goes red. A test written
after a fix has a habit of asserting nothing.

## A note for AI agents

If you are an assistant working in this repository, the same rules
apply, plus a few things that have actually gone wrong here:

- Decide Rust or Svelte before writing code, not after. The default is
  Rust.
- Do not add a dependency, a background task, a timer or a process
  without saying what it costs at rest.
- Do not implement an operation that another surface already performs.
- Measure rather than assert. If you claim something got faster or
  smaller, show the number and say how you took it.
- Summing `WorkingSetSize` across a process tree over-counts shared
  pages badly. Private working set is the honest figure.
- Some tests live in `src-tauri/tests/` because a `#[cfg(test)]` module
  in the library that constructs a `dyn Action` aborts the whole
  `cargo test --lib` run at load, naming nothing.
- Most source files here are CRLF. Match the file you are editing.

There is a longer internal version of this standard covering compute
ownership maps, cost models and a feature planning template. Everything
in it that a contributor needs is above.
