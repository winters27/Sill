//! Actions an installed extension contributes.
//!
//! Its own file rather than another entry in [`super`], and the difference is
//! not size. Everything next door is a Rust type compiled into this build, one
//! per verb, and its id is a literal. These are **built from data**: a manifest
//! read at install time says which kinds of thing a command can be run on, and
//! this turns that declaration into [`Action`]s the ordinary registry holds.
//!
//! What makes that safe is that it is the same trait and the same registry.
//! A contributed action is found by `for_kind`, drawn by `describe`, looked up
//! by `get`, and run by `ActionRegistry::perform` like everything else, so it
//! is in the activity log, it can carry a chord somebody sets in Settings, and
//! the model reaches it through the same tool. Rules 14 to 16 are one rule
//! worded three ways: **one implementation, one route to it**, and a second
//! path for extension actions would have broken all three at once.
//!
//! ## What an extension can and cannot say
//!
//! It can say which kinds its command applies to. That is the whole of the
//! declaration, and everything else follows from facts Sill already holds.
//!
//! - **The id is Sill's**, `extension.<extension>.<command>`, so an author
//!   cannot claim `sill.launch` and cannot collide with another extension.
//! - **It is never primary.** Enter on a file stays Open. Three things hold
//!   that: nothing built here claims to be, `ActionRegistry::primary` looks
//!   only at what Sill ships, and what Sill ships is searched first anyway.
//! - **It asks for no permission.** Running one starts the extension's own
//!   command with the extension's own grants; see
//!   [`super::open_extension_command`]. A contributed action is not a way to
//!   ask for something the install card did not.
//! - **It draws no screen.** Only a `no-view` command may contribute one, and
//!   installing refuses the rest by name. An action is a verb picked out of a
//!   panel, and the panel has nowhere to put a view.
//!
//! ## Why the tests for this are in `tests/actions.rs`
//!
//! They were here first, and the whole `cargo test --lib` run died at load
//! with `STATUS_ENTRYPOINT_NOT_FOUND`, naming nothing. The reason is the one
//! [`crate::suite`] already writes down: the library's own test binary does not
//! get `build.rs`'s manifest link argument, so anything retaining the dialog
//! plugin's `TaskDialogIndirect` refuses to start. A test here builds a
//! `dyn Action`, which mints a vtable holding [`Contributed::run`], which
//! reaches the extension host and its `confirmAlert`. Reproduced rather than
//! assumed, both ways round.

use async_trait::async_trait;
use tauri::Manager;

use crate::action::{Action, ActionCtx, Capability, Outcome};
use crate::object::{Object, ObjectKind};
use crate::registry::CommandRecord;

/// The one command in the index with this id.
///
/// Looked up at the moment it is run rather than held, because an extension
/// can be updated or removed between the panel being drawn and a row in it
/// being pressed, and a held record would run a bundle that is not there.
pub(crate) fn command_record(ctx: &ActionCtx, id: &str) -> Result<CommandRecord, String> {
    let registry = ctx.app.state::<crate::state::RegistryState>();
    let found = registry
        .index()
        .commands
        .iter()
        .find(|c| c.id == id)
        .cloned();

    found.ok_or_else(|| format!("no such command: {id}"))
}

/// One command an extension declared can be run on something.
pub struct Contributed {
    /// `extension.<extension>.<command>`, and the reason it is not the
    /// author's own string is above.
    id: String,
    /// What the panel shows, which is the command's own title.
    title: String,
    /// The index id of the command to run, `<extension>:<command>`.
    command: String,
    /// The kinds it declared, already resolved.
    kinds: Vec<ObjectKind>,
}

#[async_trait]
impl Action for Contributed {
    fn id(&self) -> &str {
        &self.id
    }

    fn title(&self) -> &str {
        &self.title
    }

    fn accepts(&self, kind: ObjectKind) -> bool {
        self.kinds.contains(&kind)
    }

    /// The same thing [`super::RunExtensionCommand`] declares, because it is
    /// the same act: starting somebody else's code in a worker.
    ///
    /// Not the extension's own grants, which change while Sill runs and are
    /// checked where they can be read live. What this answers is the question
    /// the automation and AI gates ask, "what does invoking this amount to",
    /// and the honest answer is that it runs a program. `ProcessLaunch` is
    /// what makes `automation::may_schedule` refuse to put one on a scheduled
    /// task, so a trigger firing with nobody there cannot start an extension.
    fn capabilities(&self) -> &'static [Capability] {
        &[Capability::ProcessLaunch]
    }

    async fn run(&self, ctx: &ActionCtx, object: &Object) -> Result<Outcome, String> {
        let record = command_record(ctx, &self.command)?;

        super::open_extension_command(ctx, &record, &self.title, Some(object)).await
    }
}

/// The prefix every contributed id carries.
///
/// Never `sill.`, which is the namespace of everything this build ships, and
/// `tests/actions.rs` holds both halves of that: nothing shipped is missing the
/// prefix, and nothing contributed has it.
pub const PREFIX: &str = "extension.";

/// Every action the installed extensions in this index contribute.
///
/// Reads the same records the search reads, so there is one answer to "what is
/// installed" rather than a list of extensions beside a list of actions with
/// nothing making them agree.
///
/// A kind this build does not know is **dropped, and the rest of the command's
/// declaration still stands**. Installing refuses an unknown kind by name, so
/// the only way one gets here is an index written by a newer Sill, and the
/// choice is between offering an action on a kind that does not exist here and
/// quietly not offering it. A command that ends up with no kinds at all
/// contributes nothing and is still perfectly runnable from the root list.
pub fn contributed(commands: &[CommandRecord]) -> Vec<std::sync::Arc<dyn Action>> {
    let mut out: Vec<std::sync::Arc<dyn Action>> = Vec::new();

    for record in commands {
        let Some(declared) = record.manifest.as_ref() else {
            continue;
        };

        if declared.acts_on.is_empty() {
            continue;
        }

        let kinds: Vec<ObjectKind> = declared
            .acts_on
            .iter()
            .filter_map(|name| ObjectKind::named(name))
            .collect();

        if kinds.is_empty() {
            continue;
        }

        out.push(std::sync::Arc::new(Contributed {
            id: format!("{PREFIX}{}.{}", record.extension, record.command),
            title: record.title.clone(),
            command: record.id.clone(),
            kinds,
        }));
    }

    out
}
