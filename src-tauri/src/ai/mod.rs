//! Asking a model something, from the launcher.
//!
//! One provider layer, reached three ways: Tab from the root list for a quick
//! question, a chat for a conversation, and an action on whatever is selected
//! so "explain this" works on a file, a clipboard entry or some text without
//! a second implementation of any of it.
//!
//! Every provider is reached with a key the person entered. None of them is
//! reached by signing into a chat subscription; see `provider.rs` for why,
//! per service.

pub mod acting;
pub mod approval;
pub mod chat;
pub mod claude_code;
pub mod openai;
pub mod provider;
pub mod tools;
