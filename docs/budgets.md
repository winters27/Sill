# What Sill is allowed to cost

Efficiency is a product requirement here, not a nice-to-have, and a
requirement nobody measures is a preference. This is what has been measured,
what it is allowed to grow to, and which test says so.

Every number was taken on one machine (Windows 11, 16 cores, 32 GB) against a
release build. They are a baseline to notice changes from, not a specification
anybody else's machine has to meet.

## Enforced

These fail a build or a device run when they are exceeded.

| What | Measured | Budget | Checked by |
| --- | --- | --- | --- |
| Ranking one keystroke, 1,500 entries | 2.2 to 3.7 ms | 20 ms release, 60 ms debug | `tests/budgets.rs` |
| Ranking cost as the corpus grows | linear | at most 40x for 8x the entries | `tests/budgets.rs` |
| Rust core, idle, home folder indexed | 22.4 MB private | 40 MB | `scripts/device-tests.ps1` |
| Rust core, at rest, nothing changing | 0 ms per 30 s | 500 ms per 30 s | `scripts/device-tests.ps1` |
| Clipboard write-ahead log | 0 after a checkpoint | at most 2 MB | `tests/clipboard_merge.rs` |
| Reading last run's file index | 24 to 77 ms | under 500 ms | `scripts/device-tests.ps1` |

The ranking budgets are measured against a corpus deliberately worse than a
real index: every title is built from the same sixteen words, so a query
matches nearly everything and the ranker does the most work it can. A real
index of the same size answers in a fraction of it. The budget is there to
catch a change in *kind*, where something that ran in microseconds starts
running in milliseconds because it grew a clone per candidate.

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

## Measured, not yet enforced

Recorded so a change is visible, without a test that would fail for reasons
outside the code.

| What | Measured |
| --- | --- |
| Whole application, idle, all processes | 281 MB private at the time of the audit |
| Whole application, a minute after startup | 228.6 MB private, 89.0 MB set |
| Working set, before and after the renderers suspend | 488.1 MB then 89.0 MB |
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
| Rust core while a home folder is being written to | 3.4 s of processor per 30 s, about a tenth of one core |

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
`scripts/measure-summon.ps1`, and written up in `docs/roadmap.md`.

- **Screen reader behaviour.** The markup follows the combobox pattern and the
  rule for when it applies is unit tested, but nothing here has been heard by
  NVDA or Narrator.
