# What Sill is allowed to cost

Efficiency is a product requirement here, not a nice-to-have, and a
requirement nobody measures is a preference. This is what has been measured,
what it is allowed to grow to, and which test says so.

Every number was taken on one machine (Windows 11, 16 cores, 32 GB) against a
release build. They are a baseline to notice changes from, not a specification
anybody else's machine has to meet.

This document is the contract and it is written by hand. The readings a
release publishes are not: [benchmark.md](benchmark.md) is generated from what
the measuring scripts wrote down, carries the machine and the build behind
every row, and says which rows have no reading at all. The budgets on it are
read from the tables below, so loosening one here loosens it there and
nowhere else.

## Where each of these runs

A budget nobody enforces is a number that was true once. A gate that measures
the wrong thing fails builds for no reason until somebody deletes it, and then
teaches everybody to ignore red on the way out. So the first question is not
what the thresholds are; it is where each one can honestly be taken.

There are three answers, and every row below is in one of them.

| Where | What belongs there | Why |
| --- | --- | --- |
| **Every push, on a shared agent** | counts, ratios, sizes, and structural facts | A hosted runner is a borrowed virtual machine with no display, no graphics hardware and neighbours competing for its cores. It cannot tell you what a frame costs. It can tell you perfectly well whether a number is zero, whether one thing grew faster than another, and whether a rule the code has to follow is still being followed. |
| **A nightly run on a machine set aside for it** | milliseconds and megabytes of a running launcher | These need a real window, a real display and an idle machine. None of them may be a required check, because a required check that depends on what else the machine was doing is a check that goes red for reasons nobody can act on. |
| **Not automated at all** | anything that means driving the launcher for minutes on somebody's desk | Some measurements cost more than they are worth to take. Where one of these can be answered from inside the process instead, it is, and the row says so. |

**The third category is not a cop-out and it was learned rather than
predicted.** Measuring memory after five hundred summons means opening and
closing the launcher for several minutes. Written as a script and run on a
working machine, it left ten launchers on somebody's desktop, because a toggle
that does not land leaves a process behind and the loop does not notice. What
the row was really asking is whether the state a summon leaves behind is
bounded, and that can be asked of the structures directly, with no window at
all, on every push. See `five_hundred_summons_leave_nothing_behind` in
`timing.rs`.

The same reasoning rules out driving the launcher with synthetic keys on a
machine somebody is using. A global hotkey cannot be aimed: pressing Sill's
summon key from a script sent it to whatever was in front, which was a browser,
which changed the tab somebody was reading. The keystroke measurement therefore
does not press anything. The launcher times itself while it is genuinely being
used and writes the readings to its log, and `measure-keystroke.ps1` reads
them.

## Enforced on every push

These fail `npm run verify`, and therefore the build, and they hold on a shared
agent as well as they hold here.

| What | Measured | Budget | Checked by |
| --- | --- | --- | --- |
| Ranking one keystroke, 1,500 entries | 2.2 to 3.7 ms | 20 ms release, 60 ms debug | `tests/budgets.rs` |
| Ranking cost as the corpus grows | linear | at most 40x for 8x the entries | `tests/budgets.rs` |
| Rust core, idle, home folder indexed | 22.4 MB private | 40 MB | `scripts/device-tests.ps1` |
| Rust core, at rest, nothing changing | 0 ms per 30 s | 500 ms per 30 s | `scripts/device-tests.ps1` |
| Clipboard write-ahead log | 0 after a checkpoint | at most 2 MB | `tests/clipboard_merge.rs` |
| Reading last run's file index | 24 to 77 ms | under 500 ms | `scripts/device-tests.ps1` |
| What a summon leaves behind, over 500 of them | nothing; the ring and the two headings stay their own size | no growth | `timing.rs`, `five_hundred_summons_leave_nothing_behind` |
| A repeating timer in the window that calls into Rust | 2, both accounted for | 0 unaccounted | `scripts/verify-source.mjs` |
| Looking inside files for `content:` | stops at whichever bound runs out first | 200 files, 512 KiB of each, 300 ms | `content.rs`, `grep_stops_at_its_file_bound` and `grep_stops_at_its_deadline` |

`content:` is the one search that reads files rather than an index, so its
bounds are the budget: there is no honest millisecond figure for it, because
what it costs depends entirely on the disk it is asked about. Each of the
three has a bad case the other two do not cover. A deadline alone opens
unbounded files on a fast disk; a file count alone waits out a slow network
drive; a byte cap alone does neither. It also stops between files the moment
another letter is typed, which is the fourth bound and the one that matters
while somebody is still typing.

The ranking budgets are measured against a corpus deliberately worse than a
real index: every title is built from the same sixteen words, so a query
matches nearly everything and the ranker does the most work it can. A real
index of the same size answers in a fraction of it. The budget is there to
catch a change in *kind*, where something that ran in microseconds starts
running in milliseconds because it grew a clone per candidate.

### The one behavioural gate, and why it is the strongest one here

**Network at rest has to be zero.** Unlike every other row, that is a claim
about what the product does rather than a number to stay under: there is no
acceptable amount of traffic from a window that has been put away. Either it is
quiet or the claim is false.

It was false. A weather widget pinned to the chin asked a service for a reading
every ten minutes for as long as the application was running, because
`setInterval` in `onMount` runs until the component is destroyed and hiding a
window destroys nothing. Six calls an hour on behalf of a window nobody could
see. The fix was `pollWhileVisible`.

Because the answer is a count and the count is zero, a shared agent can hold it
as well as any machine can, and it is checked in the shape that would have
caught the original: **a repeating timer in the window that calls into Rust has
to go through `pollWhileVisible`, or say in one place why being hidden stops
it.** Two do the second, and both entries carry their reasoning. A new widget
that polls with a bare `setInterval` fails the build.

Two things it deliberately does not do. It does not try to recognise a
`setTimeout` that reschedules itself, because that needs following the code and
a rule that guessed would be red for reasons nobody could act on; the clock
widget is exactly that shape and reaches Rust for nothing, so there is nothing
to catch. And it does not check the Rust side, where nearly every `sleep` is a
one-shot wait for something to settle and telling those from a poller needs the
call graph. What answers for Rust is the count taken on a real machine by
`scripts/measure-network.ps1`.

### A small machine ranks as fast as a large one

Worth stating plainly, because "fast launcher" usually means "fast on the
machine it was written on". **Ranking is one sequential pass and takes no
thread pool at all**, so there is no parallel speedup for a big machine to have
or a small one to miss.

Measured on the release build by pinning the process to a subset of the cores,
best of five over 1,500 entries for `"visual"`:

| Cores the process could use | Time |
| --- | --- |
| 1 | 4117 us |
| 2 | 4204 us |
| 4 | 3621 us |
| All 16 | 4710 us |

The whole spread is noise, and the sixteen-core run happens to be the slowest
of the four. A dual-core laptop answers a keystroke in the time this machine
does. What a shared build agent has less of is single-core speed and exclusive
use of it, which is why the budget is multiplied there and the measurement is
printed on every run instead.

## Enforced on a machine set aside for it

Run by `scripts/nightly.ps1`, which refuses a debug build. **None of these is a
required check**, deliberately: they depend on the display, the graphics driver
and what else the machine is doing, and a merge gate that depends on those is a
gate people learn to ignore.

| What | Measured | Budget | Checked by |
| --- | --- | --- | --- |
| Keystroke to the frame that draws the answer | not yet on a release build | 16 ms | `scripts/measure-keystroke.ps1` |
| Keystroke to the frame after that, when the pixels are certainly out | not yet on a release build | one refresh more | `scripts/measure-keystroke.ps1` |
| Extension activation, Enter to its first view | not yet on a release build | 300 ms warm, 1,200 ms cold | `scripts/measure-keystroke.ps1` reads it; the app records it |
| Network calls at rest, widgets pinned, 25 min | 0 in a 3 min and a 12 min watch on a debug build | 0 | `scripts/measure-network.ps1` |
| Cold start to the hotkey answering | 465 ms best, 505 mean | 4,000 ms | `scripts/measure-summon.ps1` |
| Summon, hotkey to being able to type | 25 ms median | 250 ms | `scripts/measure-summon.ps1` |
| Rust core, idle, home folder indexed | 22.4 MB private | 40 MB | `scripts/device-tests.ps1` |

**The keystroke budget is one frame at sixty hertz**, and that number is not
headroom over a measurement. It is the deadline the work has: an answer that
misses the frame it was typed in is a frame somebody spends looking at the old
list. A threshold set just above today's figure is a threshold that gets raised
the first time it fails, which is the failure mode this whole document exists
to avoid.

**Two numbers rather than one, because either alone misleads.** `answered` stops
when the rows are in the document and the frame that draws them has begun;
`presented` stops at the start of the next frame, which is the first moment the
pixels are certainly out. Reporting only the first would be a keystroke-to-paint
figure with the paint left out. Reporting only the second charges Sill for the
display's refresh interval and makes an instant answer look like a sixteen
millisecond one. Neither includes anything before the field heard about the
key: the keyboard, its driver, Windows and WebView2's input plumbing are
invisible from inside the page, and no figure here should be read as though
they were not.

## Measured, not yet enforced

Recorded so a change is visible, without a test that would fail for reasons
outside the code.

| What | Measured |
| --- | --- |
| Whole application, idle, all processes | 281 MB private at the time of the audit |
| Whole application, a minute after startup | 221.7 MB private, 78.1 MB set |
| Working set, before and after the renderers suspend | 488.1 MB then 78.1 MB |
| Idle CPU at steady state, whole tree | 0.00% of one core |
| Wakeups at rest, whole tree | 1,740 a minute across 7 threads, busiest 819 |
| Hidden but not yet suspended, first 15 s | 0.9 to 1.3% of one core |
| Hidden and suspended, next 45 s | 0.2 to 0.5% of one core |
| Cold start to the hotkey answering | 465 ms best, 505 mean over five runs |
| Clipboard listing, 135 entries | 11 KB of text |
| Finding Node once | about 40 ms |
| Icons remembered between runs | 298 icons, 964 KB on disk |
| Rust core before the file index existed | 11.3 MB private |
| Rust core with a whole drive indexed | 41 MB private |
| File index, home folder | 49,402 entries, 1.3 s to walk, 5.8 MB on disk |
| File index, whole C: drive | 157,772 entries, 6.2 s to walk, 16.2 MB on disk |
| File search, one query | 3 to 10 ms |
| Extension host, resident with nothing loaded | 0, it is not started until an extension is |
| Opening an extension, cold, five real ones | 516 to 534 ms, of which most is Node starting |
| Opening an extension, warm, five real ones | 36 to 114 ms |
| Memory one command holds, five real ones | 11.3 to 62.5 MB |
| Memory an empty worker holds | about 11 MB, which is the floor the rest sit on |
| Rust core while a home folder is being written to | 3.4 s of processor per 30 s, about a tenth of one core |
| Keystroke to the frame that draws it, **debug build behind a dev server** | 215 ms median, 298 ms worst, over 9 keystrokes |
| The same keystrokes to the frame after that | 219 ms median, 308 ms worst |
| Remote connections at rest, 12 minutes, three widgets pinned | 0 |
| Browser tabs, resident with the feature on | 0, nothing exists between two searches |
| Browser tabs, one query against a browser that is not running | 1.5 ms, the window list, then nothing |
| Browser tabs, one query, one window, 13 tabs | 40 to 60 ms debug, 35 of it the walk |
| Browser tabs, standing the automation client up | 0.1 to 0.2 ms of that |
| Browser tabs, the first query against a Firefox | 374 ms, once, then 54 |
| Browser tabs, what that first query costs the Firefox | about 10 MB in its window process, 85 MB across its pages, until it restarts |

### What the browser tab numbers are worth

The first two rows are the point of the design and the last two are its price.

Nothing about this feature exists between one search and the next: no handler
is registered, no tree is cached, and the UI Automation client itself is
created and released inside the call. **That client costs 0.1 ms to stand up**,
which is the measurement that decided it: holding one for the life of the
process would save a fifth of a millisecond and would be a permanent object
whose whole purpose is to be ready for something nobody has asked for yet.

The walk is where a query actually goes, and it is a cost paid inside the
browser rather than here. Asking for a node's children one sibling at a time
took 39 ms of a 43 ms read; asking for a whole level at once with the
properties attached took it to 33. It is still the bulk of the number.

**The Firefox row is a cost in somebody else's program**, which is why the
setting for it is separate and why the settings pane states the number. Firefox
keeps its accessibility engine off until a client asks, `ElementFromHandle` is
the asking, and it does not go off again: a read twenty minutes after the first
one, with none in between, cost 40 ms rather than 374, which is what says the
engine stayed up. Chromium exposes its own window either way and a read costs
it nothing beyond the read.

### What the idle numbers are worth

The working set falling from 488 MB to 89 MB is the renderers suspending
twenty seconds after the launcher is put away, and it is the single largest
number in this document. It is also the one that was silently not happening
until 2026-09-02: `TrySuspend` refuses while the page is busy, and a widget
polling once a second kept the whole renderer awake. Anything that starts
polling again undoes it, which is why the hidden rows above are measured
separately from the idle ones.

**Wakeups are measured because CPU time does not show them.** A thread that
wakes, looks at a flag and goes back to sleep costs almost nothing and is
exactly the kind of cost that hides: two of them ran for as long as dictation
was switched on and neither moved the CPU reading. The whole-tree number is too
noisy to see one thread in, which is why the busiest thread is reported beside
it: three runs of the same build gave 2,386, 3,173 and 4,398 a minute while the
threads in question accounted for about 330.

## Where the numbers came from

Anything quotable here is reproducible. The probes are ignored tests, so they
never run in a build and never slow one down.

```bash
cargo test --release --manifest-path src-tauri/Cargo.toml --test budgets measured -- --ignored --nocapture
```

```powershell
powershell -File scripts/measure-idle.ps1 -Label after
```

```powershell
# Use Sill for a minute, put it away, then ask what it cost.
pwsh -File scripts/measure-keystroke.ps1
```

```powershell
# Everything above that needs a running launcher, in one go. Refuses a debug
# build, and wants a machine nobody is using.
pwsh -File scripts/nightly.ps1
```

That one reports the settings that change what it means, rather than pinning
them: file indexing, clipboard history, dictation, snippet expansion and which
widgets are pinned each add work that is supposed to be there, and a
measurement of a machine nobody has is not worth taking.

```bash
ROOT="C:/Users/you" cargo test --release --manifest-path src-tauri/Cargo.toml --test probe_catalog -- --ignored --nocapture
```

```bash
INDEX="$APPDATA/app.winters.sill/index-cache.json" cargo test --release --manifest-path src-tauri/Cargo.toml --test compare_ranking -- --ignored --nocapture
```

What one extension costs, against a real host and the real protocol. Build the
bundles first with `npm run gate:views`, which is what puts them in
`extensions/build`.

```bash
node scripts/run-extension.mjs extensions/build/emoji/emoji.js emoji --measure \
  --grant fileRead,fileWrite,network,processLaunch \
  --assets extensions/raycast-src/extensions/emoji/assets
```

Cold is this process starting Node, the host bundle evaluating, a worker thread
being created and the extension's modules loading. Warm opens the same command
again in the host that is now up, against the worker that was spun up when the
first was claimed. They are far enough apart that reporting one as the other
would be a lie, which is why the Extensions panel shows both.

The five the view gate draws, on the machine this was written on:

| Extension | Cold | Warm | Memory once settled |
| --- | --- | --- | --- |
| `uuid-generator` `viewHistory` | 526 ms | 36 ms | 11.3 MB |
| `password-generator` `generate-random-password` | 527 ms | 39 ms | 11.4 MB |
| `kill-process` `index` | 531 ms | 50 ms | 12 to 37 MB |
| `hacker-news` `frontpage` | 525 ms | 114 ms | 15.4 MB |
| `emoji` `emoji` | 516 ms | 74 ms | 62.5 MB |

Two of those are worth reading twice. **Cold is the same for all five**,
because almost all of it is Node starting rather than anything the extension
does; the number that separates them is warm. And `kill-process` is a range
rather than a figure, because it reads the whole process table: caught mid-scan
it is 37 MB and 85% of a core, and settled it is 12 MB and idle. A reading is a
moment, which is why the panel says "now" and not "always".

## The one that is over

**The Rust core was 11.3 MB before it indexed files and is 22.4 MB after.** The
audit's target was 15 MB, written before file search had an index at all.

The 40 MB budget above accepts the larger figure rather than pretending the
target still holds. That is a decision, not an oversight: an index of the
folders somebody works in is worth ten megabytes, and the alternative measured
at 412 MB in somebody else's process. If it should be smaller, the arena is
where to look first, since one allocation per entry cost 3.4 MB of pure
overhead before it was folded into one.

## The other one that is over

**Re-indexing costs about a tenth of one core while files are changing**, not
the twentieth the pacing rule was described as giving.

The rule waits twenty times the last walk's wall-clock cost before walking
again. The walk is parallel, so 1.3 seconds of wall time on six threads is up
to 7.8 seconds of processor time, and the wait accounts for the first number
rather than the second. It is a twentieth of the machine rather than of one
core.

At rest with nothing changing it is zero, which is the number the design is
really about. The cost only appears while somebody is writing files in an
indexed folder.

Charging the full processor cost would mean two and a half minutes before a
new file could be found, which is a worse product. The answer is a smaller
unit of work rather than a longer wait: patching the index for the file that
changed instead of walking everything again. That is not done yet.

## What is not measured yet

Summon latency and cold start have moved out of this section: measured on the
release build at 25 ms median and 846 ms, held there by
`scripts/measure-summon.ps1`.

- **Every millisecond figure here is a release figure, and the keystroke rows
  are not yet among them.** The 215 ms above is a debug build served by a
  development server, which is two orders of magnitude away from the build the
  claim is about: Sill's pixel work measures 125 to 414 ms in debug against 3
  to 7 in release. It is recorded because it proves the measurement exists and
  arrives end to end, and for no other reason. **Nothing should be quoted from
  it.**
- **Extension activation to its first view has an instrument and no reading.**
  The app records it and `measure-keystroke.ps1` will report it; taking the
  number means opening an extension on a machine set aside for it.
- **The whole-tree idle memory row still has no threshold.** It is measured by
  `measure-idle.ps1` and reported rather than enforced, because what it costs
  depends on which widgets are pinned and what is indexed, and those are
  settings rather than regressions.
- **Screen reader behaviour.** The markup follows the combobox pattern and the
  rule for when it applies is unit tested, but nothing here has been heard by
  NVDA or Narrator.
