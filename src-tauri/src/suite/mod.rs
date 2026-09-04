//! The behaviour tests that used to be one Cargo binary each.
//!
//! ## Why they moved
//!
//! Every file directly under `tests/` is its own crate, and Cargo links a
//! separate executable for each. There were forty-five. That is why ordinary
//! work here runs `cargo test --lib` and nothing else: the library's own tests
//! finish in three and a half seconds, and a full run spends minutes linking
//! binaries before it executes a line.
//!
//! Which means anything living out there was run by CI and by nobody else, and
//! **three of those files had stopped compiling.** `search_excluding` grew a
//! `pinned` parameter, `ranking_memory`, `probe_switch_rank` and `probe_walk`
//! were never updated, and nothing anybody ran built them. Six ranking tests
//! and two probes had been dead for however long that was.
//!
//! Almost none of them needed a process. A test that ranks a corpus, merges two
//! lists of file hits or compares two JSON files is a function over values: it
//! gains nothing from an executable of its own and costs a link to have one.
//! They are modules of the library now, they compile into the one test binary,
//! and they run inside the same three and a half seconds as the rest.
//!
//! ## What stayed in `tests/`, and why
//!
//! | File | Reason |
//! | --- | --- |
//! | `exthost` | spawns a real Node and drives the host down real pipes |
//! | `mcp_bridge` | spawns `sill.exe --mcp` and talks to that process |
//! | `store_install`, `store_audit` | fetch from the network and run npm |
//! | `files` | needs a running Everything |
//! | `window_control` | creates a real window and pumps its messages |
//! | `expander` | installs a system-wide low-level keyboard hook |
//! | `actions` | needs the common-controls manifest, below |
//! | `budgets` | a wall-clock budget must not share a run |
//! | `icons`, `machine_index` | read this whole machine, in minutes |
//! | `compare_ranking`, `reads_the_real_files` | `#[ignore]` tools pointed at a real profile |
//! | `probes` | twenty `#[ignore]` diagnostics, now one binary rather than twenty |
//!
//! **The manifest.** `build.rs` emits `rustc-link-arg-tests`, which Cargo
//! applies to `tests/` targets and not to the library's own test binary.
//! Anything retaining the dialog plugin's `TaskDialogIndirect` therefore cannot
//! live here: the binary refuses to start at all, `STATUS_ENTRYPOINT_NOT_FOUND`
//! before a single test runs, with no hint about which import is missing. That
//! is `actions`, and it was reproduced rather than assumed.
//!
//! It is **only** `actions`. `clipboard_merge` and `clipboard_collections` both
//! carried a header saying they were integration tests "for the same reason the
//! action registry's are", and it was not true of either: both moved in and the
//! binary starts. A reason copied from a neighbouring file is not the same
//! thing as a reason.
//!
//! **The machine.** `registry` came here, but six of its tests did not: they
//! walk the real Start Menu, ask Windows for its packaged applications and
//! extract an icon per entry. Together they took `--lib` from 3.5 seconds to
//! **460**, one of them accounting for 438 on its own. They are
//! `tests/machine_index.rs` now, which is what a test that reads the whole
//! machine should always have been.

mod acl_parity;
mod clipboard_collections;
mod clipboard_merge;
mod file_merge;
mod keyword_matching;
mod ranking_memory;
mod real_games;
mod real_terminals;
mod registry;
mod settings_apply_live;
mod startup_order;
mod watch_filter;
mod window_reach;
mod windowing;
