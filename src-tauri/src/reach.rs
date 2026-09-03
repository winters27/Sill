/*!
What Sill is allowed to reach.

Two boundaries live here because they fail the same way and are crossed by the
same callers: **an address handed to the shell**, and **a file handed to the
model**. Both start as text somebody else wrote. A quicklink can arrive in an
exported file from anyone, an extension supplies the target of
`Action.OpenInBrowser` unread, and every path the model asks for came out of a
document it was told to summarise. None of those is Sill's own text, and the
old code treated all of them as if it were.

Kept in one module rather than beside each caller for the reason rule 22
gives: there were six places opening an address and each one would have grown
its own idea of what is safe. There is one idea, and it is here.
*/

use std::path::{Component, Path, PathBuf};

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

Anything else, including an application's own protocol such as `obsidian://`
or `slack://`, is refused. That is a real restriction on quicklinks and it is
deliberate: an application protocol handler is an arbitrary program invoked
with an argument, and Sill cannot tell which of those on a given machine is a
text editor and which is a shell.
*/
const OPENABLE: &[&str] = &["http", "https", "mailto", "ms-settings"];

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

        /// An application's own protocol is an arbitrary program with an
        /// argument. Refused on purpose, and this says so out loud so that
        /// loosening it has to be a decision.
        #[test]
        fn an_application_protocol_is_refused() {
            assert!(url("obsidian://open?vault=Brain").is_err());
            assert!(url("slack://channel?id=x").is_err());
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
}
