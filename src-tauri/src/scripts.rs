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

/// Pulls the `@raycast.*` lines out of a header, whatever comments it uses.
///
/// Every shell marks a comment differently, `#` in sh and PowerShell, `REM` or
/// `::` in batch, `//` in JavaScript. Rather than a table of those, this looks
/// for the marker itself and takes what follows, which works for all of them
/// and for the one somebody uses next.
fn fields(header: &str) -> Vec<(String, String)> {
    const MARKER: &str = "@raycast.";

    let mut found = Vec::new();

    for line in header.lines() {
        let Some(at) = line.find(MARKER) else {
            continue;
        };
        let rest = &line[at + MARKER.len()..];

        let mut parts = rest.splitn(2, char::is_whitespace);
        let Some(key) = parts.next().filter(|key| !key.is_empty()) else {
            continue;
        };

        found.push((key.to_string(), tidy(parts.next().unwrap_or_default())));
    }

    found
}

/// Reads a script's header, or decides it is not a command.
pub fn describe(path: &Path, header: &str) -> Option<Script> {
    let shell = Shell::of(path)?;
    let found = fields(header);

    let value = |name: &str| {
        found
            .iter()
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
