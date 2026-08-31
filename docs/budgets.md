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

## Measured, not yet enforced

Recorded so a change is visible, without a test that would fail for reasons
outside the code.

| What | Measured |
| --- | --- |
| Whole application, idle, all processes | 281 MB private at the time of the audit |
| Rust core before the file index existed | 11.3 MB private |
| Rust core with a whole drive indexed | 41 MB private |
| File index, home folder | 49,402 entries, 1.3 s to walk, 5.8 MB on disk |
| File index, whole C: drive | 157,772 entries, 6.2 s to walk, 16.2 MB on disk |
| File search, one query | 3 to 10 ms |
| Extension host, resident with nothing loaded | 0, it is not started until an extension is |
| Rust core while a home folder is being written to | 3.4 s of processor per 30 s, about a tenth of one core |

## Where the numbers came from

Anything quotable here is reproducible. The probes are ignored tests, so they
never run in a build and never slow one down.

```bash
cargo test --release --manifest-path src-tauri/Cargo.toml --test budgets measured -- --ignored --nocapture
```

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
