---
description: Plan a Sill feature the Rust-first way, before any code is written
argument-hint: [what the user should be able to do]
---

Plan this feature for Sill: **$ARGUMENTS**

Do not write code yet. Work through the sections below and show them to
the user first. If the request is a small revision rather than a feature,
say so and skip to the parts that apply rather than padding it out.

The rule everything follows: **Rust is the brain, Svelte is the
presenter, Tauri is the bridge.** Sill is meant to become very capable
while staying nearly free at rest, and those two goals fight each other.
Every section below exists to stop the second one losing.

## 1. The capability, not the UI

State what the user should be able to accomplish, in their words. Not
"add a panel for X" but "let Sill find and act on X".

Then answer: if the Svelte surface disappeared tomorrow, would this
feature still exist? For anything substantial it should.

## 2. What Sill already has

**Go and look before designing anything.** Read the actual code, do not
assume from the name. Check whether this can reuse:

- the search and ranking path
- the action registry and object kinds
- existing services and their state ownership
- settings, persistence, caching
- AI tools, MCP, extension APIs
- native Windows integration already in `src-tauri/src`

Extending a good abstraction beats adding a parallel one. If another
surface already performs this operation, use it rather than writing a
second implementation.

## 3. Compute ownership map

A table, one row per operation, before any code:

| Operation | Rust or Svelte | Why |
| --- | --- | --- |

The default is Rust. Ranking, fuzzy matching, filtering, sorting,
parsing, validation, caching, persistence, filesystem work, OS access
and action resolution all belong in `src-tauri`.

Svelte gets DOM, styling, focus, hover, selection, panel open state,
simple form state, animation, and keyboard interaction for its own
surface.

**Any non-trivial Svelte assignment needs a sentence saying why it
cannot live in Rust.** If you cannot write that sentence, it belongs in
Rust.

## 4. Cost model

Answer every line. "Unclear" means investigate before implementing, not
guess.

- **Idle** — what exists while nobody uses this? The right answer is
  usually nothing.
- **Invocation** — what happens on first use?
- **Warm** — what stays behind afterwards?
- **Memory** — what remains resident, and is it bounded?
- **CPU** — what computes, and on which path?
- **Wakeups** — does anything recur? Why can an event not do instead?
- **Network** — does it connect, and when?
- **Process** — does it spawn anything? What stops it?
- **WebView** — does it need another surface? A new one is not free.

## 5. Lifecycle

Disabled, dormant, requested, active, released, dormant.

Say how it stops. Startup code without teardown code is unfinished, and
a disabled feature should be close to zero cost: no polling, no
connections, no loaded models, no spawned processes.

## 6. The boundary

Commands, events and payloads. Design them around user intent.

Ask for the answer, not the corpus: `search(query, limit)` returning
twelve rows, not the whole index for the frontend to filter. Do not turn
a keystroke or a hover into a round trip either.

## 7. What must stay true

Performance acceptance criteria for this feature specifically. For
example: no idle polling, no persistent process, bounded cache, the
search path unchanged, a hidden window producing no activity from this
feature.

These are what gets verified at the end.

## 8. Risks

What could make the launcher slower or heavier? Name it now rather than
discovering it in review.

---

When the plan is agreed, build Rust and the domain first, then adapt it
to a surface. Verify with `npm run verify`, read the exit code before
the test counts, and break each new test to confirm it can fail.

The longer form of all of this is in `CONTRIBUTING.md`.
