/*!
Actions a configured MCP server contributes.

The sibling of [`super::extension`], and deliberately built the same way: a
declaration is read, turned into ordinary [`Action`]s, and handed to the one
registry through the one funnel. `for_kind` finds them, `describe` draws them,
`get` looks them up, and [`crate::action::ActionRegistry::perform`] runs them,
so an MCP action is in the activity log, can carry a chord somebody set in
Settings, and is reached by the model through the same tool as everything else.
Rules 14 to 16 are one rule worded three ways, and a second path for these
would have broken all three at once.

## What is different, and it is the whole design

An extension is code **Sill installed**. It was fetched from a catalogue Sill
knows, unpacked into Sill's own directory, refused at install if its manifest
said something Sill does not allow, and it runs in Sill's own worker under
grants Sill checks on every call.

An MCP server is **somebody else's program on the far end of a pipe**. Sill did
not install it, cannot inspect it, does not run it, and has no say in what it
does once it is started. It may be slow, it may hang, it may answer with
nonsense, and it may have been uninstalled since the day it was configured.
Three things follow, and each is a decision rather than an accident.

### The panel is drawn from the configuration, never from the server

Nothing in this file starts a process. [`contributed`] reads the declarations
out of the preferences and builds actions from them, so the action panel, which
is drawn on a keystroke, is a list built out of strings that were already in
memory. **A server that is dead, slow, hung or uninstalled cannot make the
launcher wait**, because at the moment the panel is drawn no server has been
asked anything.

That is why the declaration says which kinds a tool applies to rather than
Sill working it out by asking. Asking would be the obvious design and it would
put somebody else's process on the path of a keystroke, and then the honest
version of it needs a cache, and the cache needs invalidating, and a launcher
that draws its action panel in twelve milliseconds now has a spinner in it.

`verify:source` holds this: [`contributed`] may not name the client at all.

### What starts a server

Two things, both of them somebody pressing something.

- Running one of its actions. [`Contributed::run`] starts the program, does the
  handshake, calls the one tool, reads the answer and ends the process before
  it returns. Nothing is kept afterwards.
- Pressing Check in Settings, which lists a server's tools so somebody can pick
  one without reading its source.

A configured server nobody is using costs a few strings in a file. No process,
no connection, no thread, no timer, and nothing to shut down. That is rule 23
answered by construction rather than by a lifecycle somebody has to get right.

### Where it sits relative to the Windows Hello gate

**The gate does not move. The capability is what puts it where it belongs.**

`P8-03` put [`Capability::ShellExecution`] and `FileWrite` behind Hello for the
AI and MCP callers, and `P8-02` refused to move a gate when the honest fix was
to narrow what a trigger may name. The same answer applies here, and it falls
out of asking the question [`Action::capabilities`] actually asks: **what does
invoking this amount to?**

For an extension, [`super::extension::Contributed`] answers `ProcessLaunch`,
because it starts Sill's own host running code Sill installed under grants Sill
checks. None of that is true here. Invoking one of these starts an arbitrary
program on this machine, named by a command line, and hands it a string; and
`ShellExecution`'s own definition is that it "hands over a shell, and a shell is
every other capability on this list at once". So that is what it declares, and
three things follow with no new rule anywhere:

- **`automation::may_schedule` refuses it.** A trigger firing at three in the
  morning cannot start somebody's MCP server with nobody there.
- **The model reaches it through Hello**, exactly like `ShellExecution` from
  anywhere else. This is the case that matters most: a model chaining one
  server's output into another server's tool is the injection path, and it is
  the path `P8-03` exists for.
- **A `sill://` link reaches the approval card**, through `outside.rs`, because
  `needs_asking` is true of it.

A person picking the row out of the action panel runs it, and that is the line
`P8-03` already drew rather than a new one. The gate exists because a card
proves a keypress and not a person; somebody choosing a row in a launcher they
opened **is** the person, and asking them for a fingerprint to confirm the
keystroke they just made is the theatre `P8-02` refused. What guards that case
is what guards running a script: they configured the server, they can see its
name on the row, and the action is written to Activity.

## Why the tests for this are in `tests/actions.rs`

The same trap [`super::extension`] names: any `#[cfg(test)]` module in the
library that constructs a `dyn Action` mints a vtable that reaches the dialog
plugin and kills the whole `cargo test --lib` run at load, naming nothing.
*/

use async_trait::async_trait;
use tauri::Manager;

use crate::action::{Action, ActionCtx, Capability, Outcome};
use crate::object::{Object, ObjectKind};
use crate::preferences::McpServer;

/// The prefix every id from a server carries.
///
/// Never `sill.`, which is what this build ships, and never `extension.`, which
/// is what an installed extension contributes. Three namespaces, so `get`
/// cannot be made to hand back somebody else's action by naming it cleverly.
pub const PREFIX: &str = "mcp.";

/// How much of a server's answer goes in the line somebody reads.
///
/// The whole answer is still carried on [`Outcome::text`], so a shortcut can
/// put it where the text came from and a workflow can chain it. This is only
/// the sentence in the launcher, and a tool that answered with a page of JSON
/// must not become a page of JSON in a toast.
const SHOWN: usize = 300;

/// One tool a configured server offers, as an action.
pub struct Contributed {
    /// `mcp.<server>.<tool>`, minted here so nobody can claim `sill.launch`.
    id: String,
    /// What the panel shows, which is what the person called it.
    title: String,
    /// Which configured server, looked up again when it is run.
    server: String,
    /// The tool to call on it.
    tool: String,
    /// The one argument the thing being acted on is passed as.
    ///
    /// Empty for a tool that takes nothing, which is a real shape: "reload the
    /// index" applies to a file without being told which one.
    argument: String,
    /// The kinds it was declared for, already resolved.
    kinds: Vec<ObjectKind>,
}

/// The one configured server with this name, read at the moment it is run.
///
/// Looked up rather than held, for the reason [`super::extension::command_record`]
/// is: somebody can edit or remove a server between the panel being drawn and a
/// row in it being pressed, and a held copy would start a program they have
/// already taken out.
async fn configured(ctx: &ActionCtx, name: &str) -> Result<McpServer, String> {
    let prefs = ctx.app.state::<crate::state::PrefsState>();

    // Held for one clone and let go. The call below can wait a minute, and
    // holding the settings lock across that would stop everything else in Sill
    // reading them for as long as somebody else's program is thinking.
    let found = {
        let held = prefs.inner.lock().await;
        held.mcp.servers.iter().find(|s| s.name == name).cloned()
    };

    found.ok_or_else(|| format!("{name} is no longer set up as an MCP server"))
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

    /// See the header. Starting an arbitrary program named by a command line
    /// is what `ShellExecution` is for, and declaring it is what puts this
    /// behind Hello for the model and out of reach of a scheduled trigger.
    fn capabilities(&self) -> &'static [Capability] {
        &[Capability::ShellExecution]
    }

    async fn run(&self, ctx: &ActionCtx, object: &Object) -> Result<Outcome, String> {
        let server = configured(ctx, &self.server).await?;

        // One property, named in the declaration, holding what the action is
        // being run on. Deliberately not a map: every tool worth putting in an
        // action panel takes one thing, and a bag of named parameters would be
        // a shape invented for a case that does not exist yet.
        let mut arguments = serde_json::Map::new();
        if !self.argument.is_empty() {
            arguments.insert(
                self.argument.clone(),
                serde_json::Value::String(object.target.clone()),
            );
        }

        let said = crate::ai::mcp::client::call(
            crate::ai::mcp::client::Program {
                name: &server.name,
                command: &server.command,
                args: &server.args,
            },
            &self.tool,
            serde_json::Value::Object(arguments),
        )
        .await?;

        if said.is_empty() {
            return Ok(Outcome::done(format!("{} finished", self.title)));
        }

        // The whole answer travels, and only the sentence is cut. A shortcut
        // that puts a result back where the text came from wants all of it.
        Ok(Outcome::done(shortened(&said)).producing(said))
    }
}

/// One line of an answer, for the launcher to show.
///
/// The first line rather than the first three hundred characters of a
/// paragraph: a tool that answers with a document says the useful part at the
/// top, and a sentence cut mid-word across a newline reads as a bug.
fn shortened(said: &str) -> String {
    let first = said
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("");
    let first = first.trim();

    if first.chars().count() <= SHOWN {
        return first.to_string();
    }

    format!("{}\u{2026}", first.chars().take(SHOWN).collect::<String>())
}

/**
Every action the configured MCP servers contribute.

Built from the declarations and nothing else. **No process is started, no
connection is opened and nothing is asked of any server**, which is what lets
this be called on the path that also replaces the index's commands and lets the
action panel be drawn without waiting for anybody.

A kind this build does not know is dropped and the rest of the declaration
still stands, for the same reason [`super::extension::contributed`] gives: a
preferences file written by a newer Sill should cost the one action it names a
kind for, not the whole row.

A declaration that names no tool, or no kind Sill has, contributes nothing.
An action accepting nothing would sit in the settings list with a chord that
can never fire and no way to find out why.
*/
pub fn contributed(servers: &[McpServer]) -> Vec<std::sync::Arc<dyn Action>> {
    let mut out: Vec<std::sync::Arc<dyn Action>> = Vec::new();

    /*
     * Ids are minted from names a person typed, so two of them can be the
     * same. An extension cannot do this, because its name is a directory on
     * disk; here somebody can call two servers `notes` without noticing, and
     * `ActionRegistry::get` returns the first match for ever, so the second
     * one would sit in the panel running the first one's tool.
     *
     * The first wins, which is the order they are listed in. Refusing both
     * would take away the row that was working.
     */
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

    for server in servers {
        // A server with nothing to run it with is a row somebody started
        // filling in. It is kept in the settings so they can finish it, and it
        // contributes nothing until they do.
        if server.name.trim().is_empty() || server.command.trim().is_empty() {
            continue;
        }

        for offered in &server.actions {
            if offered.tool.trim().is_empty() {
                continue;
            }

            let kinds: Vec<ObjectKind> = offered
                .acts_on
                .iter()
                .filter_map(|name| ObjectKind::named(name))
                .collect();

            if kinds.is_empty() {
                continue;
            }

            let id = format!("{PREFIX}{}.{}", server.name, offered.tool);

            if !seen.insert(id.clone()) {
                continue;
            }

            out.push(std::sync::Arc::new(Contributed {
                id,
                // The tool's own name when nobody wrote a title, because a
                // blank row in the action panel is a row nobody can press on
                // purpose.
                title: if offered.title.trim().is_empty() {
                    offered.tool.clone()
                } else {
                    offered.title.clone()
                },
                server: server.name.clone(),
                tool: offered.tool.clone(),
                argument: offered.argument.trim().to_string(),
                kinds,
            }));
        }
    }

    out
}
