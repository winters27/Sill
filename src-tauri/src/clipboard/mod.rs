//! Clipboard history.
//!
//! Everything copied is kept, searchable, and pastable back into whatever has
//! focus. It is the feature every launcher in this space ships and the one
//! that gets used most after search itself.

pub mod commands;
pub mod kind;
pub mod monitor;
pub mod sensitive;
pub mod store;
