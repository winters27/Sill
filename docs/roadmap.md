# Where Sill is

What the audit of 2026-08-29 laid out, and what has actually been built since.
Kept here rather than in a planning tool because it is about this code and
should move with it.

A line is done when something checks it. Where that is a test, it is named.

## P0, foundation and correctness

| | Item | State |
| --- | --- | --- |
| P0.1 | Version control, MIT headers, CI running verify | **Repo exists** with full history. CI has no remote to run on yet |
| P0.2 | Idle pass: cap results, debounce, bound the icon cache, drop PATH executables, checkpoint the log | **Done.** Result cap is 120, icon cache evicts, PATH executables default off, the clipboard log is bounded and checkpointed |
| P0.3 | Instrument before promising | **Partly.** Ranking, idle memory and idle processor are measured and enforced. Summon latency and cold start are not |
| P0.4 | Extension correctness | **Done.** Every method the host can call is answered, storage persists, crashes reach the window |
| P0.5 | Release-mode host bundling | **Done.** Three candidates, in order, and only one that exists is returned |
| P0.6 | Credentials at rest | **Done.** Keys go to DPAPI rather than into a settings file |
| P0.7 | Split `lib.rs` | **Done.** Commands live in `commands/`, state in `state.rs` |
| P0.8 | Typed object and action model | **Done.** `object.rs`, `action.rs`, a registry with capabilities and undo |
| P0.9 | Performance budgets | **Done.** `docs/budgets.md`, `tests/budgets.rs`, and idle checks in the device suite |

## P1, best in class core

| | Item | State |
| --- | --- | --- |
| P1.1 | Ranking: hysteresis, typo tolerance, aliases | **Done.** Match classes, a gap limit, initials read as initials, explicit and learned aliases |
| P1.2 | Context captured at summon | **Done.** `ActionCtx` |
| P1.3 | Window enumeration and control | **Done.** `windowing.rs` |
| P1.4 | Window management commands | **Done.** Halves, thirds, quarters, monitors |
| P1.5 | Window switcher | **Done.** Previews are still P2 |
| P1.6 | Selection actions | **Done.** Case, tidy, title |
| P1.7 | Clipboard: merge, HTML, secrets, collections | **Done** |
| P1.8 | Navigation bindings | **Done.** Vim and Emacs presets |
| P1.9 | Command history | **Done.** Up recalls what was searched |
| P1.10 | Application and command hotkeys | **Done** |
| P1.11 | Extension install path | **Not started.** Blocked on a decision, see below |
| P1.12 | Emoji and symbols | **Done.** In the picker and in ordinary searches |
| P1.13 | Snippets: collections, rich text, app-specific, forms | **Partly.** Placeholders and import/export are done. Collections, rich text and app-specific are not |
| P1.14 | Native dictation panel | **Not started, and the reason for it was wrong.** See below |
| P1.15 | Disabled means stopped | **Done** |

## P2, power user

| | Item | State |
| --- | --- | --- |
| P2.1 | Activity history and undo | Not started. `Outcome.undo` exists and window moves use it |
| P2.2 | Screenshot | Not started |
| P2.3 | OCR on demand | Not started |
| P2.4 | Read aloud | Not started |
| P2.5 | System control | **Partly.** Volume, mute, dark mode and lock are done. Per-app volume, audio device, do not disturb, night light, wifi and bluetooth are not |
| P2.6 | Process and resource view | Not started |
| P2.7 | Terminal execution, capability gated | Not started |
| P2.8 | Scripting | Not started |
| P2.9 | File actions | **Partly.** Copy path, copy name, reveal, open a terminal, move to the recycle bin. Rename, move, compress and hash are not |
| P2.10 | Hyperkey and double-tap modifiers | Not started |
| P2.11 | Browser history and bookmarks | Not started |
| P2.12 | Workspace profiles | Not started |
| P2.13 | Live command results | Not started |
| P2.14 | Migration import | **Partly.** Snippets read a file written by other tools |
| P2.15 | Dictation retention policy | Not started |

## Built since the audit, and not on it

- **A file index of Sill's own.** File search no longer needs another program
  installed. Walks the folders somebody chooses, respects `.gitignore`, keeps
  the index between runs, and updates when files appear or go away.
- **Drives.** Whole volumes can be indexed, with the system folders left out.
- **A frontend test runner.** The window's own logic had none, which is where
  the worst bug of the week lived.
- **A device suite.** Nine checks that only a running Sill can answer.
- **A source check.** Damage that compiles: stray NUL bytes, replacement
  characters, conflict markers.

## Decisions waiting

**P1.11, the extension install path.** Extensions are TypeScript and JSX, and
Node runs neither. No bundler is needed, because the host already resolves
modules, but a transpiler is. Measured: `typescript.js` is 8.7 MB of pure
JavaScript and already vendored; `sucrase` is 1.1 MB with seven dependencies;
`esbuild` is a 10.1 MB binary **per platform**. Writing one is a real TS and
JSX parser with a long tail of silent wrongness. This is an install-size call
rather than a technical one.

**P1.14, the native dictation panel.** Its justification was that a second
window costs a second renderer. That was measured and found to be false: both
windows share one. The item should be re-scoped or dropped rather than built
for a reason that turned out not to hold.

**Re-indexing cost.** About a tenth of one core while files are changing in an
indexed folder, and zero at rest. The real answer is patching the index for the
file that changed rather than walking everything again. See `docs/budgets.md`.
