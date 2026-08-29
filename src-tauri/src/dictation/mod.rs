//! Local dictation: microphone capture, transcription, and paste.
//!
//! Audio flows one way and every stage is a separate module so the pure
//! signal-processing steps stay unit-testable without a microphone: capture
//! produces interleaved f32 at whatever rate the device negotiated,
//! `resample` brings that to the 16 kHz whisper requires, and `wav` frames it
//! as the PCM clip a transcription backend accepts.

pub mod assets;
pub mod audio;
pub mod bands;
pub mod capture;
pub mod commands;
pub mod context;
pub mod engine;
pub mod error;
pub mod fetch;
pub mod history;
pub mod hotkey;
pub mod job;
pub mod models;
pub mod panel;
pub mod paste;
pub mod provider;
pub mod providers;
pub mod resample;
pub mod server;
pub mod service;
pub mod sound;
pub mod transcriber;
pub mod wav;
