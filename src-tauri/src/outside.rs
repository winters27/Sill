/*!
Things asked of Sill by something that is not Sill.

Two callers, one path. A `sill://run/<action>?target=` address, which Windows
delivers by starting `sill.exe` with the address as its command line, and
`sill run <action> <target>` typed at a prompt, which is the same thing with
different words. Both arrive as argv, both are read by [`crate::reach`], and
both end in `ActionRegistry::perform` alongside a keypress, a bound chord, a
workflow step and a tool the model called. There is no second way to run an
action and this is not one.

## Why argv and not a socket

The item this implements said "over the existing loopback bridge", and that
bridge is the wrong door. Its whole security model is a secret minted per run
of Sill and handed to a child process in its environment, and `P0-06` went out
of its way to stop that secret ever resting on disk: the config naming it is a
guard that deletes itself when the question that needed it is over. A shell has
no way to learn a secret that deliberately does not exist anywhere a shell can
look, and making one durable enough to be looked up would undo the item that
removed it.

Argv needs no secret because it already carries the right proof. Starting
`sill.exe` is something only a process running as this user can do, which is
exactly the authority `sill run` claims to have, and it is also how Windows
hands over a protocol address. One door, one parser, one gate, rather than a
port for the command and an argument for the link.

What it costs is that neither caller gets an answer back. The single instance
plugin sends the command line to the running Sill and exits; the shell that
typed it sees an exit code and nothing else. What happened is on screen, in the
card and afterwards in the activity log, which is where somebody who just
clicked a link is looking anyway.

## What a stranger may reach

[`crate::reach::may_run`] decides, and the two lists it reads are argued there.
The short version: an address may name two actions, both of which show
somebody something, and it stops and asks every single time. A shell may name
anything in the registry and asks on the same terms the model does, because
anything able to type the command could have run the program itself.
*/

use tauri::{AppHandle, Manager};

use crate::action::{ActionCtx, ActionRegistry, Capability};
use crate::reach::{Ask, Trust};

/**
Where a refusal is filed.

One id, so a page that fires ten links leaves one line rather than ten, and so
the next link that works withdraws it. Sill may well have no window on screen
when this happens, and something clicked that visibly did nothing has to be
readable somewhere afterwards.
*/
const REFUSED: &str = "outside:refused";

/**
Whether this command line was asking for something, and starts it if so.

Answers rather than acting on both, because the caller has a second thing to do
with the answer: a launch that asked for nothing is somebody who wanted the
window they already have, and that still has to toggle.
*/
pub fn arrived(app: &AppHandle, argv: &[String]) -> bool {
    let Some(asked) = crate::reach::asked_of(argv) else {
        return false;
    };

    let app = app.clone();

    // Spawned, because this waits ninety seconds on somebody reading a card
    // and the single instance callback runs on the thread that received the
    // other process's message.
    tauri::async_runtime::spawn(async move {
        match asked {
            Ok(ask) => run(&app, ask).await,
            Err(why) => refuse(&app, why),
        }
    });

    true
}

/// Runs one, once whoever it belongs to has agreed to it.
async fn run(app: &AppHandle, ask: Ask) {
    // Copied out before anything awaits. The registry is managed state
    // borrowed from the app, and holding that borrow across an await is what
    // stops this compiling; both of these are `'static` and so survive it.
    let found = {
        let registry = app.state::<ActionRegistry>();
        registry
            .get(&ask.action)
            .map(|found| (found.title().to_string(), found.capabilities()))
    };

    let Some((title, capabilities)) = found else {
        return refuse(app, format!("Sill has no action called {}.", ask.action));
    };

    if let Err(why) = crate::reach::may_run(ask.trust, &ask.action, capabilities) {
        return refuse(app, why);
    }

    let object = match crate::ai::acting::object_for(&ask.target, ask.kind.as_deref()) {
        Ok(object) => object,
        Err(why) => return refuse(app, why),
    };

    let accepts = {
        let registry = app.state::<ActionRegistry>();
        registry
            .get(&ask.action)
            .is_some_and(|found| found.accepts(object.kind))
    };

    if !accepts {
        return refuse(app, format!("{title} cannot be done to {}.", object.title));
    }

    if asks_first(ask.trust, capabilities) {
        let pending = app.state::<crate::ai::approval::Pending>();
        let id = pending.next_id();

        crate::ai::approval::raise(
            app,
            crate::ai::approval::Asking {
                id: id.clone(),
                title: title.to_string(),
                // The whole target rather than the object's title, which for a
                // file is the name alone. Somebody deciding about a link they
                // clicked needs to see WHERE, and `notes.txt` in a folder they
                // have never opened reads exactly like their own.
                subject: shown(&ask.target),
                touches: asked_by(ask.trust, capabilities),
                // The Windows Hello gate covers what a model asks for. A
                // clicked link is a different caller with its own trust
                // levels above, so nothing stronger was withheld here and
                // there is nothing to explain on the card.
                instead: None,
            },
        );

        match pending.wait(&id).await {
            crate::ai::approval::Answer::Allowed => {}
            // Their own refusal, so it is not filed as something wrong. The
            // log is enough: the surface exists for things Sill quietly failed
            // to do, and this is Sill doing exactly what it was told.
            crate::ai::approval::Answer::Refused => {
                crate::say!("[outside] {title} was refused");
                return;
            }
            crate::ai::approval::Answer::Unanswered => {
                return refuse(
                    app,
                    format!("Nobody answered about {title}, so nothing was done."),
                );
            }
        }
    }

    let ctx = ActionCtx::answering(app.clone(), ask.argument.clone());

    let outcome = {
        let registry = app.state::<ActionRegistry>();
        let Some(found) = registry.get(&ask.action) else {
            return refuse(app, format!("Sill has no action called {}.", ask.action));
        };
        registry.perform(&ctx, found.as_ref(), &object).await
    };

    match outcome {
        Ok(outcome) => {
            crate::say!("[outside] {}", outcome.message);
            // Whatever was last refused is no longer the news.
            crate::status::resolved(app, REFUSED);
        }
        Err(why) => refuse(app, why),
    }
}

/**
Whether this stops and asks before it runs.

**A link always asks, whatever it names.** Nobody went looking for it: they
clicked something, and the card is the first they hear that a launcher on their
machine was asked to do anything at all. The capability rule the model lives
under is the wrong rule here, because it is about how much damage an action can
do, and the question a link raises is not damage but consent.

A shell asks on exactly the model's terms, and reusing that rule rather than
writing a second one is the point: a read runs, anything that writes a file,
launches a program, types, changes the machine or reaches the network stops. A
command that had to be typed by somebody already able to run the program is not
owed a second opinion about reading a file.
*/
fn asks_first(trust: Trust, capabilities: &[Capability]) -> bool {
    match trust {
        Trust::Link => true,
        Trust::Shell => crate::ai::acting::needs_asking(capabilities),
    }
}

/**
The card's one sentence about what is about to happen.

It has to name the asker. The card reads "This ..." and every other thing that
raises one is the model, in a window somebody is already talking to; this one
can arrive with nothing of Sill's on screen, and "opens something" with no
subject is a question about nothing. Somebody deciding in half a second is
deciding on whether they meant to click the thing they clicked.
*/
fn asked_by(trust: Trust, capabilities: &[Capability]) -> String {
    let touches = crate::ai::acting::what_it_touches(capabilities);

    match trust {
        Trust::Link => format!(
            "was asked for by a {}:// link, and {touches}",
            crate::reach::SCHEME
        ),
        Trust::Shell => format!("was asked for by sill run, and {touches}"),
    }
}

/// How long a target may be on the card.
///
/// Long enough for a path with a couple of folders in it, short enough that a
/// query string a mile long cannot push the buttons off the bottom of a card
/// somebody is meant to read before pressing one.
const AT_MOST: usize = 120;

/// The target, cut to something a card can carry.
fn shown(target: &str) -> String {
    if target.chars().count() <= AT_MOST {
        return target.to_string();
    }

    let short: String = target.chars().take(AT_MOST).collect();
    format!("{short}\u{2026}")
}

/// Says why nothing happened, somewhere it can be read afterwards.
fn refuse(app: &AppHandle, why: impl Into<String>) {
    crate::status::report(app, REFUSED, why, None);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The rule that makes a link a link.
    ///
    /// Nothing about the action decides it. An address naming the safest
    /// action in the registry still asks, because the question it raises is
    /// whether somebody meant to click, and no capability answers that.
    #[test]
    fn a_link_asks_whatever_it_names() {
        assert!(asks_first(Trust::Link, &[]));
        assert!(asks_first(Trust::Link, &[Capability::Ui]));
        assert!(asks_first(Trust::Link, &[Capability::FileRead]));
        assert!(asks_first(Trust::Link, &[Capability::ProcessLaunch]));
    }

    /// A shell asks on the model's terms, which is the same rule written once.
    #[test]
    fn a_shell_asks_when_something_changes() {
        assert!(asks_first(Trust::Shell, &[Capability::FileWrite]));
        assert!(asks_first(Trust::Shell, &[Capability::ShellExecution]));
        assert!(asks_first(Trust::Shell, &[Capability::SystemControl]));

        assert!(!asks_first(Trust::Shell, &[Capability::FileRead]));
        assert!(!asks_first(Trust::Shell, &[]));
    }

    /// The card must say who is asking, because for these two the asker is
    /// the entire question.
    #[test]
    fn the_card_names_who_asked() {
        let link = asked_by(Trust::Link, &[Capability::ProcessLaunch]);
        assert!(link.contains("sill:// link"), "{link}");
        assert!(link.contains("opens something"), "{link}");

        let shell = asked_by(Trust::Shell, &[Capability::FileWrite]);
        assert!(shell.contains("sill run"), "{shell}");
        assert!(shell.contains("changes files"), "{shell}");
    }

    /// A card is read before a key is pressed, and a target the length of a
    /// paragraph is a card nobody reads to the end of.
    #[test]
    fn a_very_long_target_is_cut_for_the_card() {
        let long = "x".repeat(400);
        assert!(shown(&long).chars().count() <= AT_MOST + 1);
        assert_eq!(shown(r"C:\Users\me\notes.txt"), r"C:\Users\me\notes.txt");
    }
}
