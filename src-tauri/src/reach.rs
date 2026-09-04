/*!
What Sill is allowed to reach, and what is allowed to reach Sill.

Three boundaries live here because they fail the same way and are crossed by
the same callers: **an address handed to the shell**, **a file handed to the
model**, and **an address that arrives asking Sill to do something**. All three
start as text somebody else wrote. A quicklink can arrive in an exported file
from anyone, an extension supplies the target of `Action.OpenInBrowser` unread,
every path the model asks for came out of a document it was told to summarise,
and a `sill://` link is written by whoever wrote the page it was clicked on.
None of those is Sill's own text, and the old code treated all of them as if it
were.

The third one points the other way and still belongs here. It is the same
question asked backwards: the first two are "may this address be handed to
Windows", the third is "may this address be handed to the action registry", and
answering them in two modules would be two ideas about one word. Rule 22, and
the same reason the first two are together: there were six places opening an
address and each would have grown its own idea of what is safe.
*/

use std::path::{Component, Path, PathBuf};

use crate::action::Capability;

// ------------------------------------------------------------- addresses

/**
The schemes Sill will hand to the shell.

**An allow-list, and the direction is the entire point.** A list of dangerous
schemes cannot be written down. `javascript:` and `data:` are the ones everyone
remembers; `vbscript:` still runs, `file:` reads the disk, and Windows keeps
adding handlers that turn out to execute things: `search-ms:`, `ms-msdt:` and
`ms-officecmd:` were all ordinary protocol handlers until somebody noticed
otherwise. A deny-list is a list of the attacks that have already happened.

So: four schemes, each because Sill has a feature that needs it. `http` and
`https` are what a quicklink and a web search are. `mailto` is the one address
kind that is not a page and that people genuinely keep as a quicklink.
`ms-settings` is what the Windows settings catalogue opens with, and it is a
handler that only ever shows a settings page.

Then three applications, named one at a time because they were asked for:
`obsidian`, `vscode` and `slack`. A quicklink to a note, a repository or a
channel is most of what a launcher is for, and refusing them made the feature
worse in a way nobody was asking to be protected from.

What that costs is worth being plain about. An application protocol handler is
a program invoked with an argument, so each name here trusts that program to
handle a hostile argument sensibly. They are named individually rather than by
a rule such as "any installed handler", because that rule is what turns the
list back into a deny-list of the handlers somebody has already noticed. Adding
a fourth is a one-line change and a deliberate one.

**Where a hostile argument would come from** is the thing to watch. A quicklink
somebody typed is their own business. `import_quicklinks` reads a file, and a
file can come from anywhere; that path is the reason this list is short.
*/
const OPENABLE: &[&str] = &[
    "http",
    "https",
    "mailto",
    "ms-settings",
    "obsidian",
    "vscode",
    "slack",
];

/**
The scheme of `target`, if it has one at all.

RFC 3986 exactly: a letter, then letters, digits and `+-.`, then a colon.
Deliberately strict rather than reusing a looser test, because the looseness is
where the hole is. `java\tscript:alert(1)` has no valid scheme, and a parser
that shrugs at the tab and answers `java\tscript` is one that has to remember
to normalise; a parser that answers `None` cannot forget.

**At least two characters**, which is what keeps `C:\Users` a path rather than
a URL with the scheme `c`. Windows has no one-letter protocol handlers and
every drive letter is exactly one.
*/
pub fn scheme_of(target: &str) -> Option<String> {
    let colon = target.find(':')?;
    let scheme = &target[..colon];

    if scheme.len() < 2 || !scheme.starts_with(|c: char| c.is_ascii_alphabetic()) {
        return None;
    }

    if !scheme
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
    {
        return None;
    }

    Some(scheme.to_ascii_lowercase())
}

/**
The address, if Sill may open it.

Answers with the exact string to hand to the opener rather than with a
boolean, so the thing checked and the thing opened cannot differ. Checking
`target` and then opening `target.trim()` is the same class of mistake as
checking a path and then following a link.
*/
pub fn url(target: &str) -> Result<String, String> {
    let target = printable(target)?;

    let Some(scheme) = scheme_of(&target) else {
        return Err(format!("{target} is not a web address."));
    };

    if !OPENABLE.contains(&scheme.as_str()) {
        return Err(refusal(&scheme));
    }

    Ok(target)
}

/**
The target, if Sill may hand it to the shell as either an address or a path.

Quicklinks, and the extension host's `open`, carry both: a link is a URL and a
link is also `C:\Users\Brandon\Notes`. Anything that parses as a URL answers to
the allow-list above; anything that does not is a path, and opening a path is
what the file manager does every day.
*/
pub fn target(target: &str) -> Result<String, String> {
    let target = printable(target)?;

    match scheme_of(&target) {
        Some(scheme) if !OPENABLE.contains(&scheme.as_str()) => Err(refusal(&scheme)),
        _ => Ok(target),
    }
}

/// Said the same way wherever the refusal comes from.
fn refusal(scheme: &str) -> String {
    format!(
        "Sill will not open a {scheme}: address. Only http, https, mailto and \
         ms-settings addresses are opened."
    )
}

/**
The target with the whitespace around it gone, or why it is not openable.

A control character inside is refused rather than stripped. Browsers and the
shell delete tabs and newlines out of the middle of a URL before acting on it,
which means `java\nscript:alert(1)` and `javascript:alert(1)` are the same
address to them and different strings to anything checking one. Nothing Sill
opens legitimately contains one, so refusing is both safe and honest.
*/
fn printable(target: &str) -> Result<String, String> {
    let target = target.trim();

    if target.is_empty() {
        return Err("There is nothing to open.".to_string());
    }

    if target.chars().any(char::is_control) {
        return Err("That address has a control character in it, so it is not opened.".to_string());
    }

    Ok(target.to_string())
}

// ----------------------------------------------------------------- files

/**
Where the model may read without asking.

The home directory, which is where a person's own documents are and where a
question about "my notes" is answered. Everything else on the machine is
reachable through the card in [`crate::ai::approval`], which is the same
consent every action that changes something already goes through.

Not a static. Rule 2, and it costs one environment read on a tool call that is
about to touch the disk anyway.
*/
pub fn home() -> Option<PathBuf> {
    let profile = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME"))?;
    std::fs::canonicalize(profile).ok()
}

/// What a path is allowed to be.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reading {
    /// Inside the home directory and not private. Read it.
    Freely,
    /// A real path somewhere else on the machine. Ask first.
    IfAllowed,
}

/**
Whether the model may read `path`, and whether it has to ask.

**Canonicalised before anything is compared, and the canonical form is what is
compared.** Every way out of a confined tree is a way of writing a path that
does not look like where it lands, and Windows has more of them than most:
`..`, a directory junction or symlink, an 8.3 short name (`C:\Users\BRANDO~1`),
a drive-relative path (`C:notes.txt`, which resolves against a current
directory kept per drive), and the verbatim `\\?\` and `\\?\UNC\` prefixes.
`std::fs::canonicalize` resolves all of them through the operating system,
which is the only thing that knows the answer, and hands back one shape.

A path that does not exist is refused here rather than later. That is not a
security decision, it is the reason canonicalising is possible at all, and the
message is the one the model needs either way.

There is a race between this answer and the read that follows it: a junction
swapped in between the two points somewhere else. Closing it needs the file
opened once and read through that handle, which is a larger change than this
and is written down in the report rather than pretended away.
*/
pub fn readable(path: &str) -> Result<(PathBuf, Reading), String> {
    let asked = path.trim();

    if asked.is_empty() {
        return Err("Name a file.".to_string());
    }

    if asked.chars().any(char::is_control) {
        return Err("That path has a control character in it.".to_string());
    }

    let real = std::fs::canonicalize(asked)
        .map_err(|_| format!("There is nothing at {asked}, so there was nothing to read."))?;

    let Some(home) = home() else {
        return Ok((real, Reading::IfAllowed));
    };

    if !under(&real, &home) {
        return Ok((real, Reading::IfAllowed));
    }

    if let Some(part) = private_part(&real, &home) {
        return Err(format!(
            "{part} holds credentials rather than documents, so Sill does not read inside it."
        ));
    }

    Ok((real, Reading::Freely))
}

/**
The part of a home path that is nobody's business, if there is one.

**Not a list of filenames.** Two shapes cover what is worth refusing and both
are structural rather than remembered: `AppData`, which is where every
installed program keeps its tokens, cookie databases and session state, and
any name beginning with a dot, which on Windows is how the tools ported from
Unix mark their own configuration. `.ssh`, `.aws`, `.gnupg`, `.docker`,
`.kube`, `.npmrc` and `.git-credentials` are all caught by the second without
any of them being named, and so is the one added next year.

This is a refusal inside an allow-list rather than an allow-list of its own,
and it is defence in depth rather than the boundary: the boundary is that a
path outside the home directory has to be asked about. It exists because the
most valuable secrets on a Windows machine are the ones stored *inside* the
directory a person thinks of as their documents.
*/
fn private_part(path: &Path, home: &Path) -> Option<String> {
    let depth = home.components().count();

    for part in path.components().skip(depth) {
        let name = part.as_os_str().to_string_lossy();

        if name.eq_ignore_ascii_case("AppData") || name.starts_with('.') {
            return Some(name.to_string());
        }
    }

    None
}

/**
Whether `path` is `root` or lives under it.

Component by component, never by string prefix. `C:\Users\Brandon` is a string
prefix of `C:\Users\Brandon2`, which is a different person's home directory,
and a check written with `starts_with` on the text says it is inside.

Compared without case because Windows paths are, and because both sides have
been through `canonicalize` and so should already agree: the comparison is here
so that a day when they do not agree is a refusal rather than a hole.
*/
fn under(path: &Path, root: &Path) -> bool {
    let mut want = root.components();
    let mut have = path.components();

    loop {
        match (want.next(), have.next()) {
            (None, _) => return true,
            (Some(_), None) => return false,
            (Some(a), Some(b)) if !same(a, b) => return false,
            _ => {}
        }
    }
}

fn same(a: Component, b: Component) -> bool {
    a.as_os_str()
        .to_string_lossy()
        .eq_ignore_ascii_case(&b.as_os_str().to_string_lossy())
}

// ------------------------------------------------------ reaching Sill

/// Sill's own scheme.
pub const SCHEME: &str = "sill";

/// The only thing an address may ask Sill for.
const RUN: &str = "run";

/// How the command is spelled, for the sentence that says it was spelled wrong.
pub const USAGE: &str = "sill run <action> <target> [--kind <kind>] [--argument <text>]";

/**
Who is asking Sill to run something.

The two are not the same favour and must not be one type with a flag. A
`sill://` address is written by whoever wrote the page it sits on, arrives on
one click, and carries no context at all: the person clicked a link, and
whatever happens next happens to them. `sill run` was typed into a shell that
already has every right its owner has, and anything able to type it could have
run the program itself.

So the two are separated at the door rather than at the point of use, because
"which of these is it" asked deep inside is a question somebody eventually
forgets to ask.
*/
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trust {
    /// A `sill://` address, from wherever addresses come from.
    Link,
    /// `sill run`, typed on this machine by somebody with a shell.
    Shell,
}

/// What somebody outside this process asked Sill to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ask {
    pub trust: Trust,
    /// The registry id, exactly as written. Never matched by prefix.
    pub action: String,
    /// What to do it to. For a link, already past [`target`].
    pub target: String,
    /// What sort of thing the target is, when it cannot be worked out.
    pub kind: Option<String>,
    /// The one answer an action may have had to ask for.
    pub argument: Option<String>,
}

/**
The actions a `sill://` address may name.

**An allow-list, and for a stronger reason than the scheme list above.** The
registry moves files to the recycle bin, runs scripts, empties the recycle bin,
quits processes and shuts the machine down, and every one of those is reachable
by id. A list of the ids a link may not use would be a list that a new action
joins by default, which is the wrong default for the one caller that is a
stranger.

Two names, and both of them show somebody something. Opening a file or folder
and revealing it in Explorer is what a launcher's address is for: a note in a
wiki that opens the folder it is about. Nothing on this list writes, deletes,
runs a shell, types, or changes the machine, and [`LINKED`] is what keeps that
true when one of them grows.

What it costs is worth being plain about. `sill.launch` opens whatever the
address names, so a link can put a card in front of somebody offering to open a
program they downloaded. The card names it, Windows still applies its own mark
of the web to anything that came off the internet, and the person has to say
yes; that is the whole of the defence and it is the same defence double
clicking the file has. Adding a third name here is a one-line change and a
deliberate one.
*/
pub const LINKABLE: &[&str] = &["sill.launch", "sill.revealInFolder"];

/**
Everything an action reached by a link is allowed to declare.

The second gate, and the one that survives the first going stale. [`LINKABLE`]
is a list of names, and a name says nothing about what the thing behind it
does: an action on that list that grows a `ShellExecution` next year is still
on the list, and nobody rereading the list would notice. A capability is
declared by the action itself, so this gate reads what the action actually is
rather than what it was called when somebody wrote its name down.

`Ui` is here because drawing inside Sill's own window is free everywhere else
too, and `ProcessLaunch` because opening the named thing is the entire feature.
Everything else on [`Capability`] is absent, including the reads: a link that
could reach the clipboard or the selection would be a page on the internet
reading what somebody had copied.
*/
pub const LINKED: &[Capability] = &[Capability::ProcessLaunch, Capability::Ui];

/**
Whether this asker may run this action.

**The one place the difference between a link and a shell is written down.**
Called for both, including the case that always says yes, so that the check is
part of the path rather than something a link happens to be routed through:
`verify:source` holds a `perform` in `outside.rs` to having this above it, and
a gate that only some callers reach cannot be held to anything.

A shell is not gated because there is nothing to gate. Anything that can type
`sill run` can type the program's name, and refusing it here would protect
nobody from anything while making the command useless for the half of the
registry it is actually wanted for.
*/
pub fn may_run(trust: Trust, id: &str, capabilities: &[Capability]) -> Result<(), String> {
    if trust == Trust::Shell {
        return Ok(());
    }

    if !LINKABLE.contains(&id) {
        return Err(format!(
            "Sill will not run {id} from a link. A {SCHEME}:// link may only run {}.",
            LINKABLE.join(" or "),
        ));
    }

    for capability in capabilities {
        if !LINKED.contains(capability) {
            return Err(format!(
                "Sill will not run {id} from a link, because it {}.",
                crate::ai::acting::what_it_touches(&[*capability]),
            ));
        }
    }

    Ok(())
}

/**
What a launch of `sill.exe` is asking for, if it is asking for anything.

`None` for an ordinary second launch, which is somebody who wanted the window
they already have. Takes the whole command line, program name included, because
that is what both callers hold: `std::env::args` at startup and the argv the
single instance plugin hands over.

**An address anywhere in the line makes the whole line an address**, and the
order matters. Windows starts a protocol handler as `sill.exe "%1"`, so an
address arrives as one argument; a handler somebody registered by hand without
the quotes would split an address containing a space into several. Reading the
address wherever it lands means that broken line is still read as the link it
is, rather than the leading fragment falling through to the shell form and
being trusted as if somebody had typed it.
*/
pub fn asked_of(argv: &[String]) -> Option<Result<Ask, String>> {
    let given: Vec<&str> = argv.iter().skip(1).map(String::as_str).collect();

    if let Some(address) = given
        .iter()
        .find(|given| scheme_of(given).as_deref() == Some(SCHEME))
    {
        return Some(link(address));
    }

    if given.first() == Some(&RUN) {
        return Some(typed(&given[1..]));
    }

    None
}

/// A `sill://` address, read into what it is asking for.
fn link(address: &str) -> Result<Ask, String> {
    let address = printable(address)?;

    let rest = address.split_once(':').map_or("", |(_, rest)| rest);
    let rest = rest.strip_prefix("//").unwrap_or(rest);

    let (path, query) = rest.split_once('?').unwrap_or((rest, ""));
    let mut parts = path.split('/').filter(|part| !part.is_empty());

    if parts.next() != Some(RUN) {
        return Err(format!(
            "Sill only understands {SCHEME}://{RUN}/<action>, and {address} is not that.",
        ));
    }

    let Some(action) = parts.next() else {
        return Err(format!("{address} names no action to run."));
    };

    let action = decoded(action)?;
    let Some(action) = some_text(Some(action)) else {
        return Err(format!("{address} names no action to run."));
    };

    if parts.next().is_some() {
        return Err(format!(
            "{address} has more than an action after {RUN}/, so it is not run.",
        ));
    }

    let mut target = None;
    let mut kind = None;
    let mut argument = None;

    for pair in query.split('&').filter(|pair| !pair.is_empty()) {
        let Some((name, value)) = pair.split_once('=') else {
            return Err(format!("{pair} in that link is not a name and a value."));
        };

        let value = decoded(value)?;

        // An allow-list here too, and for the ordinary reason: a name nobody
        // reads is a name somebody can put anything in, and a link that
        // carries a field Sill ignores is a link whose author believed it did
        // something.
        match name {
            "target" => target = Some(value),
            "kind" => kind = Some(value),
            "argument" => argument = Some(value),
            _ => {
                return Err(format!(
                "A {SCHEME}:// link carries target, kind and argument. It does not carry {name}.",
            ))
            }
        }
    }

    let Some(target) = some_text(target) else {
        return Err(format!(
            "{address} says nothing to act on. A link needs ?target= as well.",
        ));
    };

    // The same allow-list an address Sill opens goes through, and the answer
    // is what gets carried on, so the string checked and the string acted on
    // cannot be different strings.
    let target = self::target(&target)?;

    Ok(Ask {
        trust: Trust::Link,
        action,
        target,
        kind: some_text(kind),
        argument: some_text(argument),
    })
}

/// `sill run ...`, read into what it is asking for.
fn typed(given: &[&str]) -> Result<Ask, String> {
    let mut action: Option<String> = None;
    let mut target: Option<String> = None;
    let mut kind = None;
    let mut argument = None;
    let mut at = 0;

    while at < given.len() {
        let word = given[at];

        if word == "--kind" || word == "--argument" {
            let Some(value) = given.get(at + 1) else {
                return Err(format!("{word} was given nothing to be. {USAGE}"));
            };

            if word == "--kind" {
                kind = Some((*value).to_string());
            } else {
                argument = Some((*value).to_string());
            }

            at += 2;
            continue;
        }

        if word.starts_with("--") {
            return Err(format!("sill run does not take {word}. {USAGE}"));
        }

        if action.is_none() {
            action = Some(word.to_string());
        } else if target.is_none() {
            target = Some(word.to_string());
        } else {
            return Err(format!(
                "sill run takes one action and one target, so {word} is one thing too many. {USAGE}",
            ));
        }

        at += 1;
    }

    let (Some(action), Some(target)) = (some_text(action), some_text(target)) else {
        return Err(format!("Name an action and a thing to do it to. {USAGE}"));
    };

    Ok(Ask {
        trust: Trust::Shell,
        action: printable(&action)?,
        target: printable(&target)?,
        kind: some_text(kind),
        argument: some_text(argument),
    })
}

/// What a field amounts to once the whitespace is off it.
///
/// Blank is absent, for the reason `ActionCtx` gives: an argument whose value
/// was left empty is not an answer of `""`, and a target of `""` is not a
/// target. A query string has no absent, only empty, so everything arriving
/// from a link needs this.
fn some_text(given: Option<String>) -> Option<String> {
    given
        .map(|given| given.trim().to_string())
        .filter(|given| !given.is_empty())
}

/**
One percent-encoded field, read.

Strict about the escape itself: two characters and both hexadecimal.
`u8::from_str_radix` on its own accepts `%+f`, which no encoder produces and
which every other reader of the same address would resolve differently, and a
disagreement about what an address says is the whole of how a check gets
bypassed.

**The control character check is after the decoding and not before it**, which
is the only place it works. `%0A` is three printable characters until it is a
newline, so an address examined as written is a different string from the one
that gets acted on. `printable` above sees the address before this and catches
a raw control character; this catches the encoded one.
*/
fn decoded(text: &str) -> Result<String, String> {
    let raw = text.as_bytes();
    let mut out = Vec::with_capacity(raw.len());
    let mut at = 0;

    while at < raw.len() {
        if raw[at] != b'%' {
            out.push(raw[at]);
            at += 1;
            continue;
        }

        let digits = text.get(at + 1..at + 3).unwrap_or_default();

        if digits.len() != 2 || !digits.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(format!(
                "%{digits} in that link is not a percent escape, so the link is not read.",
            ));
        }

        let byte = u8::from_str_radix(digits, 16)
            .map_err(|_| format!("%{digits} in that link is not a percent escape."))?;

        out.push(byte);
        at += 3;
    }

    let said = String::from_utf8(out)
        .map_err(|_| "That link decodes to something that is not text.".to_string())?;

    if said.chars().any(char::is_control) {
        return Err("That link has a control character in it, so it is not run.".to_string());
    }

    Ok(said)
}

#[cfg(test)]
mod tests {
    use super::*;

    mod addresses {
        use super::*;

        #[test]
        fn the_ordinary_ones_open() {
            assert_eq!(url("https://example.com").unwrap(), "https://example.com");
            assert_eq!(url("http://example.com").unwrap(), "http://example.com");
            assert_eq!(url("mailto:a@example.com").unwrap(), "mailto:a@example.com");
            assert_eq!(url("ms-settings:display").unwrap(), "ms-settings:display");
        }

        /// The whole reason this module exists. An imported quicklink is a
        /// file somebody else wrote.
        #[test]
        fn script_addresses_are_refused() {
            for evil in [
                "javascript:alert(1)",
                "data:text/html,<script>alert(1)</script>",
                "vbscript:msgbox(1)",
                "file:///C:/Windows/System32/config/SAM",
                "about:blank",
                "search-ms:query=password",
                "ms-msdt:/id PCWDiagnostic",
            ] {
                assert!(url(evil).is_err(), "{evil} was allowed");
                assert!(target(evil).is_err(), "{evil} was allowed as a target");
            }
        }

        /// A list matched case-sensitively is not a list. The shell does not
        /// care how a scheme is spelled and neither may this.
        #[test]
        fn a_scheme_in_odd_case_is_still_that_scheme() {
            for evil in [
                "JaVaScRiPt:alert(1)",
                "JAVASCRIPT:alert(1)",
                "Data:text/html,x",
                "FILE:///C:/Windows",
            ] {
                assert!(url(evil).is_err(), "{evil} was allowed");
            }

            assert!(url("HTTPS://example.com").is_ok());
        }

        /// Deleted from the middle of a URL by everything that opens one, so
        /// a checker that leaves them in is checking a different string from
        /// the one that gets acted on.
        #[test]
        fn a_control_character_is_refused_rather_than_stripped() {
            assert!(url("java\tscript:alert(1)").is_err());
            assert!(url("java\nscript:alert(1)").is_err());
            assert!(target("java\rscript:alert(1)").is_err());
            assert!(url("https://exa\u{0}mple.com").is_err());
        }

        /// Surrounding space is trimmed, and what comes back is what must be
        /// opened, so the check and the act cannot be about different text.
        #[test]
        fn the_answer_is_the_string_to_open() {
            assert_eq!(
                url("  https://example.com  ").unwrap(),
                "https://example.com"
            );
            assert!(url("   javascript:alert(1)   ").is_err());
        }

        /// A quicklink is as often a folder as an address.
        #[test]
        fn a_path_is_a_target_but_not_a_url() {
            assert_eq!(
                target(r"C:\Users\Brandon\Notes").unwrap(),
                r"C:\Users\Brandon\Notes"
            );
            assert!(url(r"C:\Users\Brandon\Notes").is_err());
            assert_eq!(scheme_of(r"C:\Users\Brandon"), None);
            assert_eq!(scheme_of("D:/projects"), None);
        }

        #[test]
        fn nothing_is_not_an_address() {
            assert!(url("").is_err());
            assert!(url("   ").is_err());
            assert!(target("").is_err());
        }

        /// The three applications that were asked for, and no others.
        ///
        /// An application protocol handler is a program invoked with an
        /// argument, so each of these trusts that program with a hostile one.
        /// They are named one at a time for that reason, and this test is what
        /// makes adding a fourth a decision rather than a side effect.
        #[test]
        fn the_named_applications_open_and_others_do_not() {
            for allowed in [
                "obsidian://open?vault=Brain",
                "vscode://file/C:/Sill/src-tauri/src/reach.rs",
                "slack://channel?id=x",
            ] {
                assert!(url(allowed).is_ok(), "{allowed} should open");
            }

            // Not a judgement on these programs. They are simply not on the
            // list, which is what an allow-list means.
            for refused in [
                "steam://run/730",
                "zoommtg://zoom.us/join?confno=1",
                "itms-apps://",
                "ms-msdt:/id",
                "search-ms:query=x",
            ] {
                assert!(url(refused).is_err(), "{refused} should be refused");
            }
        }

        /// Widening the list did not widen it to the dangerous ones.
        #[test]
        fn the_addresses_that_run_code_are_still_refused() {
            for refused in [
                "javascript:alert(1)",
                "data:text/html,<script>alert(1)</script>",
                "vbscript:msgbox(1)",
                "file:///C:/Windows/System32/cmd.exe",
                "JaVaScRiPt:alert(1)",
            ] {
                assert!(url(refused).is_err(), "{refused} should be refused");
            }
        }
    }

    mod files {
        use super::*;

        /// A real tree under the home directory, so canonicalising has
        /// something to resolve. Named per test so two can run at once.
        fn in_home(name: &str) -> PathBuf {
            let home = home().expect("a home directory");
            let dir = home.join(format!("sill-reach-{name}"));
            std::fs::create_dir_all(&dir).expect("made");
            dir
        }

        /**
        A path as a person would type it, with no `\\?\` on the front.

        **Built as text on purpose, and the first version of these tests was
        wrong for not doing it.** `home()` canonicalises, so everything built
        on it is a verbatim path, and `PathBuf::join` NORMALISES `..` away when
        the path is verbatim, because Windows will not resolve one there. So a
        traversal assembled with `join` had already been resolved by the time
        it reached the thing being tested, and both traversal tests were
        passing on an input with no traversal in it. Removing the
        canonicalising from `readable` did not fail a single test.

        Composing the string, and stripping the prefix so it is an ordinary
        path, is what puts the `..` back where the test says it is.
        */
        fn as_typed(path: &Path, rest: &str) -> String {
            let plain = path.to_string_lossy().replace(r"\\?\", "");
            format!(r"{plain}\{rest}")
        }

        #[test]
        fn a_document_in_the_home_directory_is_read_freely() {
            let dir = in_home("plain");
            let file = dir.join("notes.txt");
            std::fs::write(&file, b"hello").expect("written");

            let (real, how) = readable(&file.to_string_lossy()).expect("readable");
            assert_eq!(how, Reading::Freely);
            assert!(real.ends_with("notes.txt"));

            std::fs::remove_dir_all(&dir).ok();
        }

        /// The answer is where the path LANDS, not how it was spelled.
        ///
        /// The one property the whole file rests on, and the one a reader
        /// would otherwise have to take on trust: a `..` in the middle is
        /// resolved away rather than carried through, so the thing compared
        /// against the home directory and the thing opened afterwards are both
        /// the real location. A comparison against text that still contains
        /// `..` is a comparison against a claim.
        #[test]
        fn the_answer_is_where_the_path_lands_not_how_it_was_written() {
            let dir = in_home("canonical");
            let file = dir.join("notes.txt");
            std::fs::write(&file, b"hello").expect("written");

            let name = dir
                .file_name()
                .expect("a name")
                .to_string_lossy()
                .to_string();
            let roundabout = as_typed(&dir, &format!(r"..\{name}\notes.txt"));
            assert!(roundabout.contains(".."), "the test lost its own traversal");

            let (real, how) = readable(&roundabout).expect("readable");
            assert_eq!(how, Reading::Freely);
            assert!(
                !real.components().any(|part| part.as_os_str() == ".."),
                "a dot-dot survived into the answer, so nothing was resolved: {real:?}"
            );
            assert_eq!(
                real,
                std::fs::canonicalize(&file).expect("canonical"),
                "the answer is not the canonical form of the file"
            );

            std::fs::remove_dir_all(&dir).ok();
        }

        /// The traversal. `home\x\..\..\..\Windows` is outside the home
        /// directory however much of it is spelled with names that are not.
        #[test]
        fn walking_out_with_dot_dot_lands_outside_and_has_to_ask() {
            let dir = in_home("traversal");
            let out = as_typed(&dir, r"..\..\..\Windows\win.ini");
            assert!(out.contains(".."), "the test lost its own traversal");

            // A machine with no `C:\Windows\win.ini` is not a failure of the
            // rule, and it is the only reason this may be skipped. Swallowing
            // every error here is what made the first version of this test
            // pass under a `readable` that had its canonicalising removed.
            if !Path::new(&out).exists() {
                std::fs::remove_dir_all(&dir).ok();
                return;
            }

            let (real, how) = readable(&out).expect("a file that is there is answered about");
            assert_eq!(how, Reading::IfAllowed, "it walked out and was not noticed");
            assert!(
                !under(&real, &home().expect("a home directory")),
                "the answer is still inside home: {real:?}"
            );
            assert!(
                !real.components().any(|part| part.as_os_str() == ".."),
                "the traversal survived into the answer: {real:?}"
            );

            std::fs::remove_dir_all(&dir).ok();
        }

        /// The one that looks like nothing. Two of these name the same file.
        #[test]
        fn a_path_in_odd_case_is_still_inside_home() {
            let dir = in_home("casing");
            let file = dir.join("notes.txt");
            std::fs::write(&file, b"hello").expect("written");

            let shouted = file.to_string_lossy().to_uppercase();
            let (_, how) = readable(&shouted).expect("readable");
            assert_eq!(how, Reading::Freely, "the same file in caps was not inside");

            std::fs::remove_dir_all(&dir).ok();
        }

        /// Where every installed program keeps its tokens, and it is inside
        /// the directory the allow-list lets the model read.
        #[test]
        fn appdata_is_refused_outright_and_not_merely_asked_about() {
            let home = home().expect("a home directory");
            let appdata = home.join("AppData").join("Roaming");
            if !appdata.exists() {
                return;
            }

            let refused = readable(&appdata.to_string_lossy()).expect_err("refused");
            assert!(refused.contains("credentials"), "{refused}");
        }

        #[test]
        fn a_dot_directory_is_refused_outright() {
            let home = home().expect("a home directory");
            let dir = home.join(".sill-reach-secrets");
            std::fs::create_dir_all(&dir).expect("made");

            let refused = readable(&dir.to_string_lossy()).expect_err("refused");
            assert!(refused.contains("credentials"), "{refused}");

            std::fs::remove_dir_all(&dir).ok();
        }

        /// A share is not a local path and is never inside a local home
        /// directory, so it can only ever be asked about.
        #[test]
        fn a_unc_path_is_never_free() {
            for unc in [
                r"\\server\share\secrets.txt",
                r"\\?\UNC\server\share\secrets.txt",
                r"\\127.0.0.1\C$\Windows\win.ini",
            ] {
                match readable(unc) {
                    Ok((_, how)) => assert_eq!(how, Reading::IfAllowed, "{unc} was read freely"),
                    Err(_) => {}
                }
            }
        }

        /// A string prefix is not a path prefix, and the difference is
        /// somebody else's home directory.
        #[test]
        fn a_sibling_that_starts_with_the_same_letters_is_not_inside() {
            assert!(under(
                Path::new(r"\\?\C:\Users\Brandon\Notes"),
                Path::new(r"\\?\C:\Users\Brandon")
            ));
            assert!(!under(
                Path::new(r"\\?\C:\Users\Brandon2\Notes"),
                Path::new(r"\\?\C:\Users\Brandon")
            ));
        }

        #[test]
        fn something_that_is_not_there_says_so() {
            let refused = readable(r"C:\nothing\here\at\all.txt").expect_err("refused");
            assert!(refused.contains("nothing at"), "{refused}");
        }

        #[test]
        fn nothing_is_not_a_path() {
            assert!(readable("").is_err());
            assert!(readable("   ").is_err());
            assert!(readable("C:\\x\0y").is_err());
        }
    }

    /// What arrives asking Sill to do something.
    ///
    /// The most dangerous text in the tree: a `sill://` address is written by
    /// whoever wrote the page it sits on, and it names an action out of a
    /// registry that moves files, runs scripts, quits processes and shuts the
    /// machine down. Every test here is about something being refused.
    mod arriving {
        use super::*;

        fn line(words: &[&str]) -> Vec<String> {
            std::iter::once("C:/Sill/sill.exe")
                .chain(words.iter().copied())
                .map(str::to_string)
                .collect()
        }

        fn read(words: &[&str]) -> Result<Ask, String> {
            asked_of(&line(words)).expect("that line asked for something")
        }

        mod the_address {
            use super::*;

            #[test]
            fn an_ordinary_one_names_an_action_and_a_target() {
                let ask = read(&[r"sill://run/sill.launch?target=C:\Users\me\Notes"])
                    .expect("a link Sill understands");

                assert_eq!(ask.trust, Trust::Link);
                assert_eq!(ask.action, "sill.launch");
                assert_eq!(ask.target, r"C:\Users\me\Notes");
                assert_eq!(ask.kind, None);
                assert_eq!(ask.argument, None);
            }

            #[test]
            fn the_fields_it_carries_come_through_decoded() {
                let ask = read(&["sill://run/sill.launch?target=C%3A%5CUsers%5Cme&kind=folder"])
                    .expect("a link Sill understands");

                assert_eq!(ask.target, r"C:\Users\me");
                assert_eq!(ask.kind.as_deref(), Some("folder"));
            }

            /// The address has to say `run`, and nothing else is a spelling of
            /// it. Anything that shrugs here is a parser somebody can walk
            /// past by naming a path Sill never meant to answer.
            #[test]
            fn an_address_that_does_not_ask_to_run_is_refused() {
                for wrong in [
                    "sill://open/sill.launch?target=x",
                    "sill://sill.launch?target=x",
                    "sill://",
                    "sill:",
                    "sill://run",
                    "sill://run/",
                    "sill://run/sill.launch/extra?target=x",
                ] {
                    assert!(read(&[wrong]).is_err(), "{wrong} was read as a request");
                }
            }

            #[test]
            fn a_link_with_nothing_to_act_on_says_so() {
                let refused = read(&["sill://run/sill.launch"]).expect_err("refused");
                assert!(refused.contains("target"), "{refused}");

                let refused = read(&["sill://run/sill.launch?target="]).expect_err("refused");
                assert!(refused.contains("target"), "{refused}");
            }

            /// A field Sill ignores is a field whose author believed it did
            /// something, which is how a link ends up doing less than the page
            /// around it claims.
            #[test]
            fn a_field_sill_does_not_carry_is_refused_rather_than_ignored() {
                let refused =
                    read(&["sill://run/sill.launch?target=x&elevated=true"]).expect_err("refused");
                assert!(refused.contains("elevated"), "{refused}");
            }

            /// The one that looks like nothing. `%0A` is three printable
            /// characters right up until it is a newline, so an address
            /// checked as written is a different string from the one used.
            #[test]
            fn a_control_character_survives_no_encoding() {
                for evil in [
                    "sill://run/sill.launch?target=C:%00%5CWindows",
                    "sill://run/sill.launch?target=x%0Ay",
                    "sill://run/sill%09launch?target=x",
                ] {
                    assert!(read(&[evil]).is_err(), "{evil} was allowed");
                }

                assert!(
                    read(&["sill://run/sill.launch?target=x\ty"]).is_err(),
                    "a raw control character was allowed",
                );
            }

            /// `from_str_radix` alone takes `%+f`, which no encoder writes and
            /// which another reader of the same address resolves differently.
            #[test]
            fn a_percent_escape_that_is_not_one_is_refused() {
                for wrong in [
                    "sill://run/sill.launch?target=x%zz",
                    "sill://run/sill.launch?target=x%2",
                    "sill://run/sill.launch?target=x%+f",
                    "sill://run/sill.launch?target=x%",
                ] {
                    assert!(read(&[wrong]).is_err(), "{wrong} was read");
                }
            }

            /// The target goes through the same allow-list an address Sill
            /// opens goes through, so a link cannot smuggle one past by
            /// arriving as a target instead of as a quicklink.
            #[test]
            fn a_target_that_runs_code_is_refused() {
                for evil in [
                    "sill://run/sill.launch?target=javascript%3Aalert(1)",
                    "sill://run/sill.launch?target=file%3A///C%3A/Windows/System32/cmd.exe",
                    "sill://run/sill.launch?target=vbscript%3Amsgbox(1)",
                    "sill://run/sill.launch?target=ms-msdt%3A/id",
                ] {
                    assert!(read(&[evil]).is_err(), "{evil} was allowed");
                }
            }

            /// A protocol handler registered by hand as `sill.exe %1` rather
            /// than `sill.exe "%1"` splits an address with a space in it. The
            /// leading fragment must not then fall through to the typed form
            /// and be trusted as something somebody sat down and wrote.
            #[test]
            fn an_address_anywhere_in_the_line_makes_the_whole_line_an_address() {
                let ask = read(&["run", "sill://run/sill.launch?target=x", "and", "more"])
                    .expect("a link");

                assert_eq!(ask.trust, Trust::Link, "a link was trusted as a shell");
                assert_eq!(ask.action, "sill.launch");
            }

            /// Somebody who wanted the window they already have.
            #[test]
            fn an_ordinary_second_launch_asks_for_nothing() {
                assert!(asked_of(&line(&[])).is_none());
                assert!(asked_of(&line(&["--some-flag"])).is_none());
                assert!(asked_of(&line(&["raycast://run/x"])).is_none());
                assert!(asked_of(&[]).is_none());
            }
        }

        mod what_a_link_may_run {
            use super::*;

            #[test]
            fn the_two_named_actions_may_run() {
                for id in LINKABLE {
                    assert!(
                        may_run(Trust::Link, id, &[Capability::ProcessLaunch]).is_ok(),
                        "{id} is on the list and was refused",
                    );
                }
            }

            /// The whole point of the item. Every one of these is a real
            /// registry id, and a page on the internet naming one must get a
            /// refusal rather than a card.
            #[test]
            fn everything_else_in_the_registry_is_refused_by_name() {
                for id in [
                    "sill.file.recycle",
                    "sill.file.move",
                    "sill.file.rename",
                    "sill.script.run",
                    "sill.system.run",
                    "sill.process.quit",
                    "sill.process.forceQuit",
                    "sill.app.uninstall",
                    "sill.pasteSnippet",
                    "sill.window.close",
                    "sill.store.remove",
                    "sill.clipboard.copy",
                ] {
                    let refused = may_run(Trust::Link, id, &[]).expect_err("refused");
                    assert!(refused.contains(id), "{refused}");
                    assert!(
                        refused.contains("sill.launch"),
                        "the refusal does not say what a link may do: {refused}",
                    );
                }
            }

            #[test]
            fn an_action_nobody_has_heard_of_is_refused() {
                assert!(may_run(Trust::Link, "", &[]).is_err());
                assert!(may_run(Trust::Link, "sill.launch.evil", &[]).is_err());
                assert!(may_run(Trust::Link, "SILL.LAUNCH", &[]).is_err());
            }

            /**
            The second gate, and the reason there are two.

            A name says nothing about what the thing behind it does. An action
            on the list that grows a `ShellExecution` next year is still on the
            list, and nobody rereading a list of names would notice. The
            capability is declared by the action itself, so this reads what the
            action is rather than what it was called when somebody wrote its
            name down.
            */
            #[test]
            fn a_listed_action_that_grew_a_capability_is_still_refused() {
                for grown in [
                    Capability::ShellExecution,
                    Capability::FileWrite,
                    Capability::SystemControl,
                    Capability::InputInjection,
                    Capability::WindowControl,
                    Capability::ControlInvoke,
                    Capability::Network,
                    Capability::SelectionRead,
                    Capability::ClipboardRead,
                    Capability::ClipboardWrite,
                    Capability::FileRead,
                    Capability::LauncherDismiss,
                ] {
                    let refused = may_run(
                        Trust::Link,
                        "sill.launch",
                        &[Capability::ProcessLaunch, grown],
                    )
                    .expect_err("refused");

                    assert!(refused.contains("sill.launch"), "{refused}");
                }
            }
        }

        mod what_a_shell_may_run {
            use super::*;

            /// A different trust level, written down in one place. Anything
            /// able to type `sill run` can type the program's name.
            #[test]
            fn a_shell_reaches_what_a_link_cannot() {
                for id in ["sill.file.recycle", "sill.script.run", "sill.system.run"] {
                    assert!(may_run(Trust::Shell, id, &[Capability::ShellExecution]).is_ok());
                    assert!(may_run(Trust::Link, id, &[Capability::ShellExecution]).is_err());
                }
            }

            #[test]
            fn an_ordinary_command_names_an_action_and_a_target() {
                let ask =
                    read(&["run", "sill.file.recycle", r"C:\Users\me\old.txt"]).expect("a command");

                assert_eq!(ask.trust, Trust::Shell);
                assert_eq!(ask.action, "sill.file.recycle");
                assert_eq!(ask.target, r"C:\Users\me\old.txt");
            }

            #[test]
            fn it_takes_a_kind_and_an_argument() {
                let ask = read(&[
                    "run",
                    "sill.file.rename",
                    r"C:\notes.md",
                    "--argument",
                    "new name.md",
                    "--kind",
                    "file",
                ])
                .expect("a command");

                assert_eq!(ask.argument.as_deref(), Some("new name.md"));
                assert_eq!(ask.kind.as_deref(), Some("file"));
            }

            /// A command spelled wrong says how it is spelled, because the
            /// person reading it is at a prompt and can fix it.
            #[test]
            fn a_command_spelled_wrong_says_how_it_is_spelled() {
                for wrong in [
                    vec!["run"],
                    vec!["run", "sill.launch"],
                    vec!["run", "sill.launch", "x", "--kind"],
                    vec!["run", "sill.launch", "x", "--elevated"],
                    vec!["run", "sill.launch", "x", "y"],
                ] {
                    let refused = read(&wrong).expect_err("refused");
                    assert!(refused.contains("sill run"), "{refused}");
                }
            }
        }
    }
}
