# Working on Sill

Read [CONTRIBUTING.md](CONTRIBUTING.md) first. It is the architecture,
and it is short. This page is the operational detail underneath it.

## The rule everything else follows

> Rust is the brain. Svelte is the presenter. Tauri is the bridge.

Decide where an operation belongs **before** writing it, and default to
Rust. Ranking, parsing, filtering, caching, persistence, OS work and
action resolution are Rust. Svelte draws, captures input and holds
ephemeral interaction state.

Sill is meant to be very capable and nearly free at rest. Before adding
anything that runs, ask what it costs when nobody is using it. The best
answer is nothing.

## Commands

```bash
npm run verify           # everything, before any commit
npm run verify:rust:fast # cargo test --lib, the quick loop
npm run verify:source    # source-shape checks, catches design-system drift
npm run check            # svelte-check
npm run gate:views       # loads real Raycast extensions
npm run extensions:fetch # needed once on a fresh clone for the gate
```

Run the dev build with `tauri dev`, never a hand-rolled dev server.

## Traps that have actually cost time here

**Read the exit code before the summary.** `npm run verify` can print
`426 tests passed` and zero failures and still exit non-zero, because a
different part of the suite failed or never ran.

**Close Sill before running the Rust suite.** A running `sill.exe` holds
the binary, and cargo fails with `failed to remove file`. Exit code 101
with no test failures is almost always this.

**Most files are CRLF.** Match the file you are editing, or the diff
becomes the whole file.

**Tests that build a `dyn Action` go in `src-tauri/tests/`.** A
`#[cfg(test)]` module in the library that constructs one aborts the
entire `cargo test --lib` run at load, naming nothing.

**Tauri denies a command missing from `capabilities/` silently.** No
error, no log, the page renders fine and the feature is dead. A `.catch`
does not help: a denied command *resolves*. Validate the shape of what
comes back rather than trusting the type parameter.

**State is managed between `build()` and `run()`.** Tauri creates every
window before the `setup` hook runs, so state created in `setup` is
already reachable and racing.

**`window.emit` reaches every window.** Scope by putting the target
label in the payload when a message is about one window.

**Never sum `WorkingSetSize` across a process tree.** It counts shared
pages once per process, and a WebView tree shares most of its code. Use
private working set.

## Verification

A test is worth what it catches. When you add or change one, break the
thing it covers and confirm it goes red. Restore with a copy, never
`git checkout`, which has discarded uncommitted work here before.

If a sabotage still passes, that is information: either a second guard
caught it, or the test is asserting less than its name claims. Find out
which before moving on.

Do not claim something is faster, smaller or fixed without a number and
the command that produced it.

## Design system

`src/lib/theme/theme.css` owns every size and colour. Literal pixel font
sizes and inline accent colours fail `npm run verify:source`. The accent
is for selection, matches, focus and affirmative state, nothing else.

Restraint is the house style: no glows, 1px separators, no bordered chip
buttons, no toast spam, and do not surface internal machinery as UI.

## Scope

Do the work asked. When you notice something else worth fixing, say so
rather than folding it into the same change. Unrelated fixes belong in
their own commit with their own reasoning.
