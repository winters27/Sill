//! Quicklinks: a saved target, opened with something typed into it.
//!
//! What separates a launcher from a bookmark bar. A bookmark goes to one
//! page; a quicklink is a page with a hole in it, and the launcher is already
//! the place you are typing.

pub mod commands;
pub mod resolve;
pub mod store;

pub use store::Quicklink;
pub mod transfer;
