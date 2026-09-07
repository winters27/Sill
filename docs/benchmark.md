# What Sill costs

Sill is meant to be almost free to leave running, and that is a claim about
numbers rather than a feeling. This page is those numbers. Every row says what
was measured, on which machine, in which build and on what day, and under each
table is the command that takes those readings on your own machine. Where a
cost has no reading, or no command that would take one, the row says that
instead.

Generated for version 0.1.4 on 2026-09-03. Nothing on this page is written by
hand: it is assembled from what the measuring scripts concluded, and the build
refuses a copy that has been edited.

**Three readings are provisional and cannot be compared to the budgets beside
them.** A development build and a release build are two orders of magnitude
apart in the part of this that draws pixels, so a reading taken on anything
other than a release build proves the measurement arrives and nothing more.
Each one is marked in its own row.

**12 costs have no measurement yet, of the 18 here.** They are listed at the
bottom with what would take one. A page that showed only the rows with
flattering numbers on them would not be worth reading.

## Costs that mean the same on any machine

Counts, ratios and sizes. None of them depends on how fast the machine is or
what else it was doing, so these are checked on every build, on whatever
hardware happens to run it, and a reader gets the same answer.

| What it means | Reading | Allowed | Taken |
| --- | --- | --- | --- |
| Working out what to show for one letter typed, against an index of 1,500 things | 27.1 ms for the worst of four queries over 1,500 entries (provisional) | 20 ms release, 60 ms debug | 2026-09-03, debug build, machine A, **version 0.1.0** |
| Whether that cost grows in step with the index rather than faster than it | still linear | at most 40x for 8x the entries | 2026-09-03, debug build, machine A, **version 0.1.0** |
| Whether opening and closing the launcher five hundred times leaves anything behind | the check holds | no growth | 2026-09-03, debug build, machine A, **version 0.1.0** |
| Repeating timers in the window with nothing saying why being put away stops them | 0 unaccounted for, of 4 repeating timers in the window | 0 unaccounted | 2026-09-03, debug build, machine A, **version 0.1.0** |
| How large the clipboard's write-ahead file is allowed to get | **not measured yet** | at most 2 MB | never |

Take these yourself:

```bash
cargo test --release --manifest-path src-tauri/Cargo.toml --test budgets measured -- --ignored --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test budgets
cargo test --manifest-path src-tauri/Cargo.toml --lib five_hundred_summons_leave_nothing_behind
node scripts/verify-source.mjs
```

- **Machine A:** Windows 11 Pro, 16 cores, 32 GB

## Costs that need a machine nobody is using

Milliseconds and megabytes of a running launcher. These need a real window, a
real display and a machine that is not doing anything else, so they are taken
on one machine set aside for them rather than wherever a build happens to run.
Reading them off a busy machine measures the machine.

| What it means | Reading | Allowed | Taken |
| --- | --- | --- | --- |
| From a letter reaching the search field to the rows for it being in the document | 215.3 ms median, 298.0 ms worst, over 9 keystrokes from 5 visits (provisional) | 16 ms | 2026-09-03, debug build, machine A, **version 0.1.0** |
| The same letter, to the first moment its pixels are certainly on the screen | 218.7 ms median, 308.1 ms worst (provisional) | one refresh more | 2026-09-03, debug build, machine A, **version 0.1.0** |
| From pressing Enter on an extension to its first view being on the screen | **not measured yet** | 300 ms warm, 1,200 ms cold | never |
| From pressing the summon key to being able to type | **not measured yet** | 250 ms | never |
| From starting the program to the summon key answering | **not measured yet** | 4,000 ms | never |
| How many times Sill reaches off the machine while nobody is using it | **not measured yet** | 0 | never |
| What the Rust core holds while idle, with a home folder indexed | **not measured yet** | 40 MB | never |
| Processor time the Rust core spends while nothing is changing | **not measured yet** | 500 ms per 30 s | never |
| Reading last run's file index at startup, so searching works before the walk finishes | **not measured yet** | under 500 ms | never |
| Processor the whole application uses while it sits there with nobody touching it | **not measured yet** | no budget, reported so a change shows | never |
| What the whole application holds while idle, the window and Rust together | **not measured yet** | no budget, reported so a change shows | never |
| How far memory falls once the window is put away and its renderers suspend | **not measured yet** | no budget, reported so a change shows | never |
| How often it wakes up at rest, which processor time does not show | **not measured yet** | no budget, reported so a change shows | never |

Take these yourself:

```bash
pwsh -File scripts/measure-keystroke.ps1
pwsh -File scripts/measure-summon.ps1
pwsh -File scripts/measure-network.ps1
pwsh -File scripts/device-tests.ps1 -Only idle
pwsh -File scripts/device-tests.ps1 -Only cache
pwsh -File scripts/measure-idle.ps1
```

- **Machine A:** Windows 11 Pro, 16 cores, 32 GB

## Costs with no measurement yet

Named rather than left out. Having no reading is a different thing from
costing nothing, and a row that says which is which is worth more than a page
with only the flattering rows on it.

| What it means | What would measure it |
| --- | --- |
| How large the clipboard's write-ahead file is allowed to get | **nothing yet:** the test named as holding this budget is not in the tree any more |
| From pressing Enter on an extension to its first view being on the screen | **nothing yet:** the app times this and its readings come back under a tenth of a millisecond, which cannot be Enter to something on a screen, so nothing is published from it |
| From pressing the summon key to being able to type | `scripts/measure-summon.ps1`, which has not written one down |
| From starting the program to the summon key answering | `scripts/measure-summon.ps1`, which has not written one down |
| How many times Sill reaches off the machine while nobody is using it | `scripts/measure-network.ps1`, which has not written one down |
| What the Rust core holds while idle, with a home folder indexed | `scripts/device-tests.ps1`, which has not written one down |
| Processor time the Rust core spends while nothing is changing | `scripts/device-tests.ps1`, which has not written one down |
| Reading last run's file index at startup, so searching works before the walk finishes | `scripts/device-tests.ps1`, which has not written one down |
| Processor the whole application uses while it sits there with nobody touching it | `scripts/measure-idle.ps1`, which has not written one down |
| What the whole application holds while idle, the window and Rust together | `scripts/measure-idle.ps1`, which has not written one down |
| How far memory falls once the window is put away and its renderers suspend | `scripts/measure-idle.ps1`, which has not written one down |
| How often it wakes up at rest, which processor time does not show | `scripts/measure-idle.ps1`, which has not written one down |

## How a reading gets onto this page

A measuring script decides its own verdict and writes it to
`docs/measurements/`, carrying the machine, the build, the day and the version
it was taken against. This page is assembled from those files and from
`scripts/benchmarks.json`, which holds what Sill claims and what each cost is
allowed to be. Budgets are decided and so they are written down; readings are
taken and so they are never written down.

That split is what makes the page checkable. No number here can be improved by
editing this file: `npm run verify` regenerates it and fails if what it
produces is not what is committed.

```bash
node scripts/benchmark-page.mjs
```
