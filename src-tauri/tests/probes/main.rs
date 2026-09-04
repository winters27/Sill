//! The hand-run probes, in one binary instead of twenty.
//!
//! Every one of these is `#[ignore]`. They are diagnostics somebody runs by
//! name at a real desktop: what the audio endpoints are, what OCR reads off a
//! screenshot, how long a preview costs, which radios this machine has. None
//! of them runs in CI and none of them ever will, because the answer depends
//! on the machine.
//!
//! They were twenty files directly under `tests/`, and Cargo links a separate
//! executable for every file there. So twenty binaries were linked on every
//! `cargo test` in order never to be run. A subdirectory is not a target, so
//! moving them into `probes/` and naming them here makes them modules of one
//! binary and leaves the behaviour identical: same names, same `#[ignore]`,
//! same way of running them.
//!
//! ```text
//! cargo test --manifest-path src-tauri/Cargo.toml --test probes \
//!   probe_ocr -- --ignored --nocapture
//! ```
//!
//! They stayed out of the library for the reason the rest of the suite moved
//! into it, read the other way round. The library's test binary is what a
//! person runs between two edits, and twenty modules of machine-poking in it
//! would be twenty more things to compile for a run that never executes one of
//! them.

mod probe_actions;
mod probe_app_volume;
mod probe_ask_local;
mod probe_audio;
mod probe_browsers;
mod probe_capture;
mod probe_catalog;
mod probe_drive;
mod probe_icons;
mod probe_media;
mod probe_move_across_drives;
mod probe_ocr;
mod probe_patch;
mod probe_preview_cost;
mod probe_processes;
mod probe_radios;
mod probe_rich_snippet;
mod probe_setting_icons;
mod probe_switch_rank;
mod probe_tts;
mod probe_walk;
