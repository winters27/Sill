//! Script commands: files on disk that the launcher can run and find.
//!
//! ## Raycast's own header, read rather than reinvented
//!
//! A script command is an ordinary script with a block of comments at the top
//! saying what it is called and how its output should be shown:
//!
//! ```text
//! #!/bin/bash
//! # @raycast.schemaVersion 1
//! # @raycast.title Say hello
//! # @raycast.mode fullOutput
//! # @raycast.packageName Demo
//! # @raycast.icon 👋
//! # @raycast.argument1 { "type": "text", "placeholder": "name" }
//! ```
//!
//! Reading that format rather than inventing one means the scripts somebody
//! already wrote work here unchanged, which is the entire reason to support a
//! format at all. Nothing is written back, so a script stays a file that works
//! anywhere.
//!
//! ## What makes a file a command, and what leaves it alone
//!
//! Two things, both required: a `title` and a `mode`. A file in a scanned
//! folder with no header is a file somebody keeps there, not a command they
//! forgot to name, and offering to run it would turn a scripts folder into a
//! list of things nobody meant to be one keystroke away.
//!
//! The extension has to name an interpreter too. `Shell::of` returns `None`
//! for anything unknown rather than guessing, so a `.txt` in the same folder
//! is never handed to PowerShell.
//!
//! ## The three things Raycast's format has no word for
//!
//! A folder to run in, environment variables, and administrator rights. They
//! are written under Sill's own marker rather than borrowed onto Raycast's,
//! so a header carrying them still parses everywhere else and nothing here
//! has to guess which product a `@raycast.` line was meant for:
//!
//! ```text
//! # @sill.workingDirectory ../repo
//! # @sill.environment DEPLOY_ENV=staging
//! # @sill.environment REGION=eu-west-1
//! # @sill.needsAdministrator true
//! ```
//!
//! **A header is a request, not a grant.** A script file is somebody else's
//! writing: it arrives in a zip, in a checkout, in a folder that somebody
//! shares. The folder and the variables it asks for only ever reach the one
//! child process, so a script asking for them can do nothing it could not
//! already do by writing the same two lines in its own body. Administrator
//! rights are the opposite: they are a boundary rather than a convenience, so
//! the header can only ask, and the answer lives in Sill's own preferences
//! where no script can write it. See [`plan`].

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::shell::Shell;

/// How much of a file is read looking for a header.
///
/// The header is at the top by definition. Reading the whole of a large file
/// to discover it has no header is work for nothing, on every file in every
/// scanned folder, every scan.
const HEADER_BYTES: usize = 8 * 1024;

/// What happens to the output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Mode {
    /// Shown in full, in its own panel.
    FullOutput,
    /// The last line, in passing.
    Compact,
    /// Nothing shown unless it fails.
    Silent,
    /// The last line, in the list where the command was.
    Inline,
}

impl Mode {
    fn of(said: &str) -> Option<Self> {
        Some(match said.trim() {
            "fullOutput" => Self::FullOutput,
            "compact" => Self::Compact,
            "silent" => Self::Silent,
            "inline" => Self::Inline,
            _ => return None,
        })
    }
}

/// One thing the script is asked for before it runs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Argument {
    #[serde(default)]
    pub placeholder: String,
    #[serde(default)]
    pub optional: bool,
    /// Raycast's own spelling. Kept so a copied header parses unchanged.
    #[serde(default, rename = "percentEncoded")]
    pub percent_encoded: bool,
}

/// A script somebody can run from the launcher.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Script {
    pub path: PathBuf,
    pub title: String,
    pub mode: Mode,
    pub shell: Shell,
    pub package: Option<String>,
    pub icon: Option<String>,
    pub description: Option<String>,
    pub author: Option<String>,
    pub arguments: Vec<Argument>,
    /// Whether it needs somebody to type something before it runs.
    pub needs_argument: bool,
    /// The folder it asked to run in, exactly as its header wrote it.
    ///
    /// Kept unresolved so what the header said and what Sill decided stay two
    /// separate facts. A relative path is relative to the script's own folder,
    /// which [`plan`] is what settles; resolving it here would mean resolving
    /// it against whatever directory the process happened to be sitting in.
    pub directory: Option<PathBuf>,
    /// What it asked to be run with, in the order it asked.
    ///
    /// A pair list rather than a map: the header has an order, a person
    /// reading the header reads it in that order, and a map would sort it into
    /// a different one for no reason anybody could see.
    pub environment: Vec<(String, String)>,
    /// Whether its header asked for administrator rights.
    ///
    /// Asked, not granted. Nothing runs elevated because a file said so; see
    /// [`plan`].
    pub wants_admin: bool,
}

/// Strips whatever closes a block comment off the end of a value.
///
/// A header written inside PowerShell's `<# ... #>` or a C-style `/* ... */`
/// leaves the terminator sitting on the value, and `mode` reading `silent #>`
/// is not a mode, so the whole file stops being a command over punctuation.
fn tidy(value: &str) -> String {
    let mut value = value.trim();

    for closer in ["#>", "*/", "-->"] {
        if let Some(shorter) = value.strip_suffix(closer) {
            value = shorter.trim_end();
        }
    }

    value.to_string()
}

/// Raycast's own marker, read unchanged so a copied header works here.
const THEIRS: &str = "@raycast.";

/// Sill's own marker, for the things Raycast's format cannot say.
///
/// Separate rather than more `@raycast.` keys, because a header travels: a
/// script carrying `@raycast.needsAdministrator` would be making a claim about
/// a product that never defined it, and a reader would have no way to tell
/// which of the two was meant to answer.
const OURS: &str = "@sill.";

/// Pulls the marked lines out of a header, whatever comments it uses.
///
/// Every shell marks a comment differently, `#` in sh and PowerShell, `REM` or
/// `::` in batch, `//` in JavaScript. Rather than a table of those, this looks
/// for the marker itself and takes what follows, which works for all of them
/// and for the one somebody uses next.
fn fields(header: &str, marker: &str) -> Vec<(String, String)> {
    let mut found = Vec::new();

    for line in header.lines() {
        let Some(at) = line.find(marker) else {
            continue;
        };
        let rest = &line[at + marker.len()..];

        let mut parts = rest.splitn(2, char::is_whitespace);
        let Some(key) = parts.next().filter(|key| !key.is_empty()) else {
            continue;
        };

        found.push((key.to_string(), tidy(parts.next().unwrap_or_default())));
    }

    found
}

/// Whether a string can be the name of an environment variable.
///
/// Windows will take very nearly anything, which is the problem: a name with a
/// space in it can be set and can never be read back by the ordinary `%NAME%`,
/// so a header line that looked like it worked silently did nothing. An empty
/// name is not a name at all, and a NUL ends the environment block early,
/// taking every variable after it with it.
///
/// `=` cannot appear, because the split is on the first one and that is what
/// makes the name the part in front of it.
fn a_name(said: &str) -> bool {
    !said.is_empty() && !said.contains(|c: char| c.is_whitespace() || c == '\0')
}

/// The variables a header declared, in the order it declared them.
///
/// **The first line for a name wins.** A header is read top to bottom by the
/// person deciding whether to keep the file, and a second line quietly
/// overriding the first is how a header stops meaning what it appears to say:
/// appending one line to the bottom of a reviewed script would be enough.
///
/// Everything after the first `=` is the value, untouched. A value is never a
/// path, a number, or anything else Sill has an opinion about, and trimming or
/// unquoting it would be Sill editing somebody's data on the way past.
fn environment(found: &[(String, String)]) -> Vec<(String, String)> {
    let mut declared: Vec<(String, String)> = Vec::new();

    for (key, said) in found {
        if key != "environment" {
            continue;
        }

        let Some((name, value)) = said.split_once('=') else {
            continue;
        };
        let name = name.trim();

        if !a_name(name) || value.contains('\0') {
            continue;
        }

        // Windows matches an environment name without regard to case, so
        // `Path` and `PATH` are one variable and the second is the duplicate.
        if declared
            .iter()
            .any(|(had, _)| had.eq_ignore_ascii_case(name))
        {
            continue;
        }

        declared.push((name.to_string(), value.to_string()));
    }

    declared
}

/// Reads a script's header, or decides it is not a command.
pub fn describe(path: &Path, header: &str) -> Option<Script> {
    let shell = Shell::of(path)?;
    let found = fields(header, THEIRS);
    let ours = fields(header, OURS);

    let value = |name: &str| {
        found
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.clone())
            .filter(|value| !value.is_empty())
    };

    let mine = |name: &str| {
        ours.iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.clone())
            .filter(|value| !value.is_empty())
    };

    // Both required. A file with neither is a file, not a command somebody
    // forgot to finish naming.
    let title = value("title")?;
    let mode = Mode::of(&value("mode")?)?;

    let mut arguments = Vec::new();

    // Numbered from one, and a gap ends the list: Raycast's own format has
    // argument1 through argument3, and a header that jumps from 1 to 3 is a
    // mistake to stop at rather than a hole to fill with a blank.
    for n in 1..=3 {
        let Some(raw) = value(&format!("argument{n}")) else {
            break;
        };
        let Ok(parsed) = serde_json::from_str::<Argument>(&raw) else {
            break;
        };
        arguments.push(parsed);
    }

    let needs_argument = arguments.iter().any(|argument| !argument.optional);

    Some(Script {
        path: path.to_path_buf(),
        title,
        mode,
        shell,
        package: value("packageName"),
        icon: value("icon"),
        description: value("description"),
        author: value("author"),
        arguments,
        needs_argument,
        directory: mine("workingDirectory").map(PathBuf::from),
        environment: environment(&ours),
        // Exactly `true`, and nothing else counts. `yes`, `1` and `on` are all
        // reasonable guesses at what somebody meant, and guessing at the one
        // field that ends in a UAC prompt is the wrong place to be generous.
        wants_admin: mine("needsAdministrator").as_deref() == Some("true"),
    })
}

/// Where a script runs, what it runs with, and whether it runs elevated.
///
/// Decided once, here, so the launcher and the action registry cannot answer
/// the question differently. Everything in it is settled: the folder exists
/// and is a folder, and `elevated` is what Sill is going to do rather than
/// what the file asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    pub directory: PathBuf,
    pub environment: Vec<(String, String)>,
    pub elevated: bool,
}

/// A path in the one spelling that two spellings of it agree on.
///
/// Canonicalised, so a path through `..`, a short 8.3 name and a junction all
/// settle to one string, and lowercased because Windows matches paths without
/// regard to case. A path that no longer exists cannot be canonicalised and
/// falls back to what it says, which is right: an allowance for a file that is
/// gone should match nothing but itself.
fn settled(path: &Path) -> String {
    std::fs::canonicalize(path)
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .to_lowercase()
}

/// Whether this exact script is one the person has allowed to elevate.
fn allowed(path: &Path, may_elevate: &[String]) -> bool {
    let want = settled(path);
    may_elevate
        .iter()
        .any(|one| settled(Path::new(one.trim())) == want)
}

/**
Settles everything about one run before anything is started.

## Why administrator rights are not something a file may ask for

A script header is somebody else's writing. It arrives in a checkout, in a
zip, in a folder two people share. If `@sill.needsAdministrator true` were
enough on its own then adding one comment line to any file in a scanned folder
would be enough to put a UAC prompt in front of somebody, and **the prompt
Windows shows names `powershell.exe`, not the script**. There is nothing on
that dialog to tell one of fifty files apart, and the person is being asked to
decide in the half second after pressing a key they meant for the launcher.

So the header only asks. The answer is a list of script paths in Sill's own
preferences, written by the settings window and by nothing else: not by a
script, not by an extension, and not by the model, which has no tool that
writes a preference at all. A script that asks and has not been allowed does
not run.

**It does not quietly run unelevated instead.** A script that says it needs
administrator rights and is given none does half of what it was written to do,
and finding out which half is somebody's afternoon.

## Why an elevated script may not also set variables

Windows does not carry them. An elevated process is started by the AppInfo
service, which builds a fresh environment out of the profile, so the directory
survives the trip and the variables do not. A script that read one and found
it empty would take the branch nobody tested, with administrator rights, which
is the worst place for that to happen. Refused rather than silently dropped.
*/
pub fn plan(script: &Script, may_elevate: &[String]) -> Result<Plan, String> {
    let title = &script.title;

    let beside = script
        .path
        .parent()
        .filter(|folder| !folder.as_os_str().is_empty())
        .ok_or_else(|| format!("Sill cannot tell which folder {title} is in"))?;

    // An absolute declared path replaces the script's folder and a relative
    // one is read from it, which `join` gives for free. Reading a relative
    // path any other way means reading it against whatever directory this
    // process happens to be sitting in, which is not a place anybody chose.
    let directory = match &script.directory {
        Some(said) => beside.join(said),
        None => beside.to_path_buf(),
    };

    if !directory.is_dir() {
        let shown = directory.display();

        // Said rather than handed on. Windows answers a bad working directory
        // with "The directory name is invalid. (os error 267)", which names
        // neither the directory nor the script.
        return Err(match (&script.directory, directory.exists()) {
            (Some(_), true) => {
                format!("{title} asks to run in {shown}, which is a file rather than a folder")
            }
            (Some(_), false) => {
                format!("{title} asks to run in {shown}, and there is no such folder")
            }
            (None, _) => format!("{title} is in {shown}, and that folder has gone"),
        });
    }

    let environment = script.environment.clone();

    if script.wants_admin {
        if !allowed(&script.path, may_elevate) {
            // The path, not only the title. Allowing one means finding it in a
            // file picker, and a person who has just been told "Deploy" still
            // has to work out which of three files that is.
            return Err(format!(
                "{title} asks to run as administrator. Sill will not do that until {} is \
                 allowed in Settings, under Scripts.",
                script.path.display(),
            ));
        }

        if !environment.is_empty() {
            return Err(format!(
                "{title} asks to run as administrator and to set environment variables. \
                 Windows builds a fresh environment for an elevated process, so those values \
                 would not arrive; take one or the other out of its header.",
            ));
        }
    }

    Ok(Plan {
        directory,
        environment,
        elevated: script.wants_admin,
    })
}

/// Reads the top of a file and describes it, or leaves it alone.
pub fn read(path: &Path) -> Option<Script> {
    // The extension is checked before the file is opened, so a folder of
    // documents costs a directory listing rather than a read each.
    Shell::of(path)?;

    let mut file = std::fs::File::open(path).ok()?;
    let mut head = vec![0u8; HEADER_BYTES];

    use std::io::Read;
    let read = file.read(&mut head).ok()?;
    head.truncate(read);

    describe(path, &String::from_utf8_lossy(&head))
}

/// What a script asks to be told, in its author's words.
///
/// The placeholder from the header, because a script asking for "branch"
/// should say "branch". A declared argument with no placeholder still has to
/// be asked for, and "argument 2" is at least honest about which one it is;
/// leaving it blank would be a prompt with nothing above it.
pub fn asks(script: &Script) -> Vec<String> {
    script
        .arguments
        .iter()
        .enumerate()
        .map(|(at, argument)| match argument.placeholder.trim() {
            "" => format!("argument {}", at + 1),
            said => said.to_string(),
        })
        .collect()
}

/// Every script command in these folders, one level deep.
///
/// Not recursive on purpose. A scripts folder with a `node_modules` in it
/// would otherwise be a scan of tens of thousands of files on every refresh,
/// and a script command is a thing somebody drops in a folder rather than
/// files away in a tree.
pub fn scan(folders: &[PathBuf]) -> Vec<Script> {
    let mut found = Vec::new();

    for folder in folders {
        let Ok(entries) = std::fs::read_dir(folder) else {
            continue;
        };

        for entry in entries.flatten() {
            let path = entry.path();

            if path.is_file() {
                if let Some(script) = read(&path) {
                    found.push(script);
                }
            }
        }
    }

    // One order however the filesystem answers, so the list does not shuffle
    // between runs.
    found.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()));
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    const HELLO: &str = r#"#!/bin/bash
# @raycast.schemaVersion 1
# @raycast.title Say hello
# @raycast.mode fullOutput
# @raycast.packageName Demo
# @raycast.icon 
# @raycast.description Greets somebody
# @raycast.author winters
# @raycast.argument1 { "type": "text", "placeholder": "name" }

echo "hello $1"
"#;

    #[test]
    fn it_reads_a_raycast_header_whole() {
        let script = describe(Path::new("hello.sh"), HELLO).expect("a command");

        assert_eq!(script.title, "Say hello");
        assert_eq!(script.mode, Mode::FullOutput);
        assert_eq!(script.shell, Shell::Bash);
        assert_eq!(script.package.as_deref(), Some("Demo"));
        assert_eq!(script.description.as_deref(), Some("Greets somebody"));
        assert_eq!(script.author.as_deref(), Some("winters"));
        assert_eq!(script.arguments.len(), 1);
        assert_eq!(script.arguments[0].placeholder, "name");
        assert!(script.needs_argument);
    }

    /// Every shell marks a comment differently and the header is the same in
    /// all of them, so the marker is what is looked for rather than the
    /// comment style in front of it.
    #[test]
    fn the_comment_style_does_not_matter() {
        for header in [
            "# @raycast.title A\n# @raycast.mode silent",
            ":: @raycast.title A\n:: @raycast.mode silent",
            "REM @raycast.title A\nREM @raycast.mode silent",
            "// @raycast.title A\n// @raycast.mode silent",
            "<# @raycast.title A #>\n<# @raycast.mode silent #>",
        ] {
            let script = describe(Path::new("go.ps1"), header);
            assert!(script.is_some(), "not read: {header}");
            assert_eq!(script.unwrap().title, "A");
        }
    }

    mod what_is_not_a_command {
        use super::*;

        /// A file with no header is a file somebody keeps in that folder.
        ///
        /// Offering to run it would turn a scripts folder into a list of
        /// things nobody meant to be one keystroke from running.
        #[test]
        fn a_script_with_no_header_is_left_alone() {
            assert!(describe(Path::new("go.sh"), "echo hi\n").is_none());
        }

        #[test]
        fn a_header_with_no_title_is_not_a_command() {
            assert!(describe(Path::new("go.sh"), "# @raycast.mode silent").is_none());
        }

        #[test]
        fn a_header_with_no_mode_is_not_a_command() {
            assert!(describe(Path::new("go.sh"), "# @raycast.title A").is_none());
        }

        /// An unknown mode is not a command either.
        ///
        /// Defaulting it would pick how somebody's output is shown on their
        /// behalf, and getting that wrong on a `silent` script means printing
        /// something they deliberately hid.
        #[test]
        fn an_unknown_mode_is_refused_rather_than_defaulted() {
            assert!(describe(
                Path::new("go.sh"),
                "# @raycast.title A\n# @raycast.mode loud"
            )
            .is_none());
        }

        /// A perfectly good header on a file nothing can run is still not a
        /// command, or a note in a scripts folder becomes a command as soon as
        /// it quotes one.
        #[test]
        fn a_file_with_no_interpreter_is_not_a_command() {
            let header = "# @raycast.title A\n# @raycast.mode silent";
            assert!(describe(Path::new("notes.txt"), header).is_none());
            assert!(describe(Path::new("go.ps1"), header).is_some());
        }
    }

    mod what_it_asks_for {
        use super::*;

        fn with(header: &str) -> Script {
            describe(Path::new("go.sh"), header).expect("a command")
        }

        /// The author's own word, because "branch" tells somebody what to type
        /// and "argument 1" does not.
        #[test]
        fn the_placeholder_is_the_prompt() {
            let script = with(concat!(
                "# @raycast.title A
",
                "# @raycast.mode silent
",
                "# @raycast.argument1 { \"type\": \"text\", \"placeholder\": \"branch\" }
",
            ));

            assert_eq!(asks(&script), vec!["branch".to_string()]);
        }

        /// A declared argument with no placeholder is still asked for.
        ///
        /// Skipping it would run the script a parameter short, and a blank
        /// prompt is a field with nothing above it.
        #[test]
        fn one_without_a_placeholder_is_still_named() {
            let script = with(concat!(
                "# @raycast.title A
",
                "# @raycast.mode silent
",
                "# @raycast.argument1 { \"type\": \"text\" }
",
                "# @raycast.argument2 { \"type\": \"text\", \"placeholder\": \"to\" }
",
            ));

            assert_eq!(
                asks(&script),
                vec!["argument 1".to_string(), "to".to_string()]
            );
        }

        #[test]
        fn a_script_declaring_none_asks_nothing() {
            assert!(asks(&with(
                "# @raycast.title A
# @raycast.mode silent"
            ))
            .is_empty());
        }
    }

    #[test]
    fn an_optional_argument_does_not_make_it_ask() {
        let header = concat!(
            "# @raycast.title A\n",
            "# @raycast.mode silent\n",
            "# @raycast.argument1 { \"type\": \"text\", \"placeholder\": \"x\", \"optional\": true }\n",
        );

        let script = describe(Path::new("go.sh"), header).expect("a command");
        assert_eq!(script.arguments.len(), 1);
        assert!(!script.needs_argument);
    }

    /// The three things Raycast's format has no word for.
    mod what_sill_adds {
        use super::*;

        fn with(lines: &str) -> Script {
            let header = format!("# @raycast.title A\n# @raycast.mode silent\n{lines}");
            describe(Path::new("go.ps1"), &header).expect("a command")
        }

        #[test]
        fn a_header_can_name_all_three() {
            let script = with(concat!(
                "# @sill.workingDirectory C:\\Work\\repo\n",
                "# @sill.environment DEPLOY_ENV=staging\n",
                "# @sill.environment REGION=eu-west-1\n",
                "# @sill.needsAdministrator true\n",
            ));

            assert_eq!(
                script.directory.as_deref(),
                Some(Path::new(r"C:\Work\repo"))
            );
            assert_eq!(
                script.environment,
                vec![
                    ("DEPLOY_ENV".to_string(), "staging".to_string()),
                    ("REGION".to_string(), "eu-west-1".to_string()),
                ],
            );
            assert!(script.wants_admin);
        }

        /// A Raycast header on its own declares none of them.
        ///
        /// The default has to be the quiet one: the script's own folder, no
        /// variables, and no UAC prompt. Every script anybody already wrote is
        /// one of these.
        #[test]
        fn a_header_that_names_none_of_them_asks_for_nothing() {
            let script = describe(Path::new("hello.sh"), HELLO).expect("a command");

            assert_eq!(script.directory, None);
            assert!(script.environment.is_empty());
            assert!(!script.wants_admin);
        }

        /// Sill's marker is read on its own, so `@raycast.needsAdministrator`
        /// is a line about a product that never defined it and means nothing.
        #[test]
        fn the_other_marker_does_not_grant_it() {
            assert!(!with("# @raycast.needsAdministrator true\n").wants_admin);
        }

        /// Exactly `true`. Guessing at the one field that ends in a UAC prompt
        /// is the wrong place to be generous about what somebody meant.
        #[test]
        fn only_the_word_true_asks_for_administrator_rights() {
            for said in ["yes", "1", "on", "True", "TRUE", "false", ""] {
                assert!(
                    !with(&format!("# @sill.needsAdministrator {said}\n")).wants_admin,
                    "{said:?} was read as asking",
                );
            }

            assert!(with("# @sill.needsAdministrator true\n").wants_admin);
        }

        mod the_variables {
            use super::*;

            fn env(lines: &str) -> Vec<(String, String)> {
                with(lines).environment
            }

            /// Everything after the first `=` is the value, untouched.
            #[test]
            fn a_value_is_whatever_follows_the_first_equals() {
                assert_eq!(
                    env("# @sill.environment ODD=a&b \"q\" %PATH% x=y\n"),
                    vec![("ODD".to_string(), "a&b \"q\" %PATH% x=y".to_string())],
                );
            }

            /// A line with nothing to split on is not a variable.
            #[test]
            fn a_line_with_no_equals_sets_nothing() {
                assert!(env("# @sill.environment JUST_A_NAME\n").is_empty());
            }

            /// Windows will happily set a name with a space in it, and nothing
            /// can ever read it back with the ordinary `%NAME%`. A line that
            /// looked like it worked and did nothing is worse than a refusal.
            #[test]
            fn a_name_nothing_could_read_back_is_refused() {
                assert!(env("# @sill.environment TWO WORDS=x\n").is_empty());
                assert!(env("# @sill.environment =x\n").is_empty());
            }

            /// The first line for a name wins.
            ///
            /// Appending one line to the bottom of a reviewed script would
            /// otherwise be enough to change what a line at the top means.
            #[test]
            fn the_first_line_for_a_name_wins() {
                assert_eq!(
                    env(concat!(
                        "# @sill.environment TOKEN=first\n",
                        "# @sill.environment token=second\n",
                    )),
                    vec![("TOKEN".to_string(), "first".to_string())],
                    "the second line overrode the first, or was kept beside it",
                );
            }
        }
    }

    /// What is settled before anything runs.
    mod planning_a_run {
        use super::*;

        /// A real script on disk, because `plan` asks the filesystem.
        fn script(dir: &Path, name: &str, lines: &str) -> Script {
            let path = dir.join(name);
            std::fs::write(
                &path,
                format!("# @raycast.title Deploy\n# @raycast.mode silent\n{lines}"),
            )
            .expect("wrote the script");

            read(&path).expect("a command")
        }

        fn temp() -> tempfile::TempDir {
            tempfile::tempdir().expect("a temp dir")
        }

        #[test]
        fn a_script_runs_beside_itself_by_default() {
            let dir = temp();
            let plan = plan(&script(dir.path(), "go.ps1", ""), &[]).expect("a plan");

            assert_eq!(plan.directory, dir.path());
            assert!(plan.environment.is_empty());
            assert!(!plan.elevated);
        }

        /// A relative folder is read from the script's own folder, not from
        /// whatever directory this process happens to be sitting in.
        #[test]
        fn a_relative_folder_is_read_from_the_script() {
            let dir = temp();
            let inner = dir.path().join("a folder");
            std::fs::create_dir(&inner).expect("made it");

            let script = script(dir.path(), "go.ps1", "# @sill.workingDirectory a folder\n");
            let plan = plan(&script, &[]).expect("a plan");

            assert_eq!(plan.directory, inner);
        }

        /// The two spellings that break a command line, and neither is on one.
        #[test]
        fn a_folder_with_a_space_and_a_trailing_backslash_is_taken_as_written() {
            let dir = temp();
            let inner = dir.path().join("a folder");
            std::fs::create_dir(&inner).expect("made it");

            let said = format!("{}\\", inner.display());
            let script = script(
                dir.path(),
                "go.ps1",
                &format!("# @sill.workingDirectory {said}\n"),
            );

            let plan = plan(&script, &[]).expect("a plan");

            assert_eq!(plan.directory, PathBuf::from(said));
            assert!(plan.directory.is_dir(), "the trailing backslash broke it");
        }

        /// The message a person can act on.
        ///
        /// Windows answers a bad working directory with "The directory name is
        /// invalid. (os error 267)", which names neither the folder nor the
        /// script and reads as a fault in the script's own code.
        #[test]
        fn a_folder_that_is_not_there_names_itself_and_the_script() {
            let dir = temp();
            let script = script(dir.path(), "go.ps1", "# @sill.workingDirectory nowhere\n");

            let why = plan(&script, &[]).expect_err("refused");

            assert!(why.contains("Deploy"), "it did not name the script: {why}");
            assert!(why.contains("nowhere"), "it did not name the folder: {why}");
            assert!(why.contains("no such folder"), "it said {why}");
            assert!(!why.contains("os error"), "it handed on the number: {why}");
        }

        #[test]
        fn a_file_where_a_folder_should_be_says_which_it_is() {
            let dir = temp();
            std::fs::write(dir.path().join("notes.txt"), b"x").expect("wrote");
            let script = script(dir.path(), "go.ps1", "# @sill.workingDirectory notes.txt\n");

            let why = plan(&script, &[]).expect_err("refused");

            assert!(why.contains("file rather than a folder"), "it said {why}");
        }

        mod administrator_rights {
            use super::*;

            const ASKS: &str = "# @sill.needsAdministrator true\n";

            /// The header asks; it does not grant.
            ///
            /// A script file arrives in a checkout, in a zip, in a shared
            /// folder. If one comment line were enough, adding one comment
            /// line to any file in a scanned folder would put a UAC prompt in
            /// front of somebody, and the prompt names `powershell.exe`.
            #[test]
            fn a_header_alone_is_refused() {
                let dir = temp();
                let script = script(dir.path(), "go.ps1", ASKS);

                let why = plan(&script, &[]).expect_err("refused");

                assert!(why.contains("Deploy"), "it did not name the script: {why}");
                assert!(
                    why.contains(&script.path.display().to_string()),
                    "it did not say which file to allow: {why}",
                );
                assert!(why.contains("Settings"), "it did not say where: {why}");
            }

            /// Allowing a different script is not allowing this one.
            #[test]
            fn allowing_another_script_allows_nothing_here() {
                let dir = temp();
                let script = script(dir.path(), "go.ps1", ASKS);
                let other = dir.path().join("other.ps1").display().to_string();

                assert!(plan(&script, &[other]).is_err());
            }

            /// Allowing the folder is not allowing what is in it.
            ///
            /// It would be allowing every file dropped into it afterwards,
            /// which is the standing grant this list exists to avoid.
            #[test]
            fn allowing_the_folder_allows_nothing_in_it() {
                let dir = temp();
                let script = script(dir.path(), "go.ps1", ASKS);
                let folder = dir.path().display().to_string();

                assert!(plan(&script, &[folder]).is_err());
            }

            #[test]
            fn a_script_named_in_the_list_runs_elevated() {
                let dir = temp();
                let script = script(dir.path(), "go.ps1", ASKS);
                let named = script.path.display().to_string();

                let plan = plan(&script, &[named]).expect("a plan");

                assert!(plan.elevated);
            }

            /// Windows matches a path without regard to case, and a path
            /// written through `..` is the same file. An allowance that only
            /// matched one spelling would look like it had been ignored.
            #[test]
            fn a_different_spelling_of_the_same_file_still_counts() {
                let dir = temp();
                let inner = dir.path().join("scripts");
                std::fs::create_dir(&inner).expect("made it");
                let script = script(&inner, "go.ps1", ASKS);

                let roundabout = dir
                    .path()
                    .join("scripts")
                    .join("..")
                    .join("scripts")
                    .join("GO.PS1")
                    .display()
                    .to_string();

                assert!(plan(&script, &[roundabout]).expect("a plan").elevated);
            }

            /// A script that is not asking does not become elevated because
            /// its path is on the list. The list is permission, not intent.
            #[test]
            fn being_on_the_list_is_not_a_reason_to_elevate() {
                let dir = temp();
                let script = script(dir.path(), "go.ps1", "");
                let named = script.path.display().to_string();

                assert!(!plan(&script, &[named]).expect("a plan").elevated);
            }

            /// Windows builds a fresh environment for an elevated process, so
            /// a declared variable would silently not arrive. Refused rather
            /// than dropped: a script reading one and finding it empty takes
            /// the branch nobody tested, with administrator rights.
            #[test]
            fn asking_for_both_is_refused_rather_than_half_done() {
                let dir = temp();
                let script = script(
                    dir.path(),
                    "go.ps1",
                    "# @sill.needsAdministrator true\n# @sill.environment TOKEN=x\n",
                );
                let named = script.path.display().to_string();

                let why = plan(&script, &[named]).expect_err("refused");

                assert!(why.contains("fresh environment"), "it said {why}");
            }
        }
    }

    #[test]
    fn scanning_a_folder_finds_the_commands_and_nothing_else() {
        let dir = std::env::temp_dir().join("sill-scripts-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("made");

        std::fs::write(dir.join("hello.sh"), HELLO).expect("wrote");
        std::fs::write(dir.join("notes.txt"), HELLO).expect("wrote");
        std::fs::write(dir.join("bare.sh"), "echo hi\n").expect("wrote");

        let found = scan(&[dir.clone()]);

        assert_eq!(
            found.len(),
            1,
            "found {:?}",
            found.iter().map(|s| &s.title).collect::<Vec<_>>()
        );
        assert_eq!(found[0].title, "Say hello");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
