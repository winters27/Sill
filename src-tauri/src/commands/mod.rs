//! The IPC surface, split by what each part is for.
//!
//! Every function here is a transport adapter and nothing more: it takes what
//! the window sent, hands the work to whatever owns that behaviour, and turns
//! the answer into something serialisable. Anything that filters, ranks,
//! parses, caches or decides belongs behind one of the services it calls,
//! not in the command.
//!
//! Split by domain rather than gathered into one file, because a single
//! module holding the whole surface is the thing that becomes impossible to
//! reason about, and it does so gradually enough that nobody notices.

pub mod ai;
pub mod diagnostics;
pub mod extensions;
pub mod launch;
pub mod search;
pub mod settings;
pub mod system;
