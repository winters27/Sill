//! The terminals this machine actually has, and the profiles inside them.
//!
//! Before this, "Open in Terminal" tried `wt.exe`, `powershell.exe` and
//! `cmd.exe` in that order and took the first that started. That is right for
//! a fallback and wrong as the whole answer: somebody with six Windows
//! Terminal profiles, one of them the WSL distribution they actually work in,
//! got whichever one Terminal calls default.
//!
//! ## The two things that are read, and why each is awkward
//!
//! **Windows Terminal's `settings.json` is JSON with comments.** The file
//! Terminal itself writes contains `//` lines and trailing commas, because its
//! own parser accepts them. `serde_json` does not, so a plain read of a real
//! settings file fails on almost every machine that has ever opened the
//! settings UI.
//!
//! **`wsl -l -v` answers in UTF-16.** It is one of the few console programs on
//! Windows that does, and reading it as UTF-8 gives a string with a NUL
//! between every letter, which then matches nothing and looks like no
//! distributions are installed.

use serde::Serialize;

/// One thing that can open a terminal.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    /// What it calls itself, which is what somebody would search for.
    pub name: String,
    /// Whether it is Windows Terminal's idea of the default.
    pub default: bool,
}

/// Strips the comments and trailing commas Windows Terminal writes.
///
/// Not a JSONC parser, and does not need to be: it walks the text once,
/// keeping track of whether it is inside a string, and only removes what is
/// outside one. A `//` inside a value like `"https://example.com"` therefore
/// survives, which a line-based strip would eat.
pub fn without_comments(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    let mut in_string = false;
    let mut escaped = false;

    while let Some(c) = chars.next() {
        if in_string {
            out.push(c);
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_string = false;
            }
            continue;
        }

        match c {
            '"' => {
                in_string = true;
                out.push(c);
            }
            '/' if chars.peek() == Some(&'/') => {
                for skipped in chars.by_ref() {
                    if skipped == '\n' {
                        out.push('\n');
                        break;
                    }
                }
            }
            '/' if chars.peek() == Some(&'*') => {
                chars.next();
                let mut last = '\0';
                for skipped in chars.by_ref() {
                    if last == '*' && skipped == '/' {
                        break;
                    }
                    last = skipped;
                }
            }
            _ => out.push(c),
        }
    }

    drop_trailing_commas(&out)
}

/// Removes a comma that is followed by a closing brace or bracket.
///
/// Terminal writes them and `serde_json` refuses them. Only outside strings,
/// for the same reason as above.
fn drop_trailing_commas(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut in_string = false;
    let mut escaped = false;
    let chars: Vec<char> = text.chars().collect();

    for (at, &c) in chars.iter().enumerate() {
        if in_string {
            out.push(c);
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_string = false;
            }
            continue;
        }

        if c == '"' {
            in_string = true;
            out.push(c);
            continue;
        }

        if c == ',' {
            // Look forward past whitespace for the closer.
            let next = chars[at + 1..].iter().find(|c| !c.is_whitespace());
            if matches!(next, Some('}') | Some(']')) {
                continue;
            }
        }

        out.push(c);
    }

    out
}

/// The profiles in a Windows Terminal settings document.
///
/// Hidden profiles are left out: Terminal hides the ones it generated and the
/// person chose not to keep, and offering them back is offering something they
/// have already refused.
pub fn profiles_in(settings: &str) -> Vec<Profile> {
    let Ok(document) = serde_json::from_str::<serde_json::Value>(&without_comments(settings))
    else {
        return Vec::new();
    };

    let default = document
        .get("defaultProfile")
        .and_then(|one| one.as_str())
        .unwrap_or_default()
        .to_string();

    // Terminal has written both shapes: a bare list, and an object with the
    // list under `list`. Both are still in the wild.
    let list = document
        .get("profiles")
        .and_then(|one| one.get("list").or(Some(one)))
        .and_then(|one| one.as_array())
        .cloned()
        .unwrap_or_default();

    let mut seen: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();

    list.iter()
        .filter(|one| one.get("hidden").and_then(|h| h.as_bool()) != Some(true))
        .filter_map(|one| {
            let name = one.get("name")?.as_str()?.trim();
            if name.is_empty() {
                return None;
            }

            // Two profiles may genuinely share a name: this machine has
            // "Developer Command Prompt for VS 2022" twice, one per installed
            // instance. Offering both is offering a choice nobody can tell
            // apart, and `wt -p` takes the first match anyway, so the list
            // says what launching would do.
            if !seen.insert(name.to_ascii_lowercase()) {
                return None;
            }

            let guid = one.get("guid").and_then(|g| g.as_str()).unwrap_or_default();

            Some(Profile {
                name: name.to_string(),
                default: !default.is_empty() && guid == default,
            })
        })
        .collect()
}

/// The distributions in the output of `wsl -l -q`.
///
/// Takes the text already decoded, so the decoding is somebody else's problem
/// and this stays testable.
///
/// `-q` rather than `-v`, because `-v` puts a header and a running-state
/// column in the way and the state is not a reason to hide a distribution: one
/// that is stopped starts when it is opened.
pub fn distributions_in(listing: &str) -> Vec<String> {
    listing
        .lines()
        // The default distribution is marked with a leading asterisk on some
        // versions even under `-q`.
        .map(|line| line.trim().trim_start_matches('*').trim())
        .filter(|line| !line.is_empty())
        // A header slips through on some builds whatever the flags say.
        .filter(|line| !line.starts_with("Windows Subsystem for Linux"))
        .filter(|line| !line.eq_ignore_ascii_case("NAME"))
        .map(str::to_string)
        .collect()
}

/// What to hand `wt.exe` to open a folder, in a named profile or the default.
///
/// Returned as separate arguments rather than a command line, because
/// `Command::arg` quotes each one for the runtime to read back and building a
/// line by hand is how `P0-11` let `x&calc` run a second command. A profile
/// name is somebody else's text: this machine has one containing spaces and
/// nothing stops one containing a quote.
pub fn wt_arguments(profile: Option<&str>, folder: &str) -> Vec<String> {
    let mut args = Vec::new();

    if let Some(profile) = profile.map(str::trim).filter(|one| !one.is_empty()) {
        args.push("-p".to_string());
        args.push(profile.to_string());
    }

    // Always last, and always its own argument. `wt` reads `-d` as the
    // starting directory for the profile named before it.
    args.push("-d".to_string());
    args.push(folder.to_string());
    args
}

/// Decodes console output that may be UTF-16.
///
/// `wsl.exe` answers in UTF-16 little endian, which is unusual enough that
/// reading it as UTF-8 does not fail: it produces a string with a NUL between
/// every letter, which matches nothing and reads as "no distributions".
///
/// Detected by the byte order mark, or by the NUL that a UTF-16 encoding of
/// ASCII always puts in the second byte.
pub fn console_text(bytes: &[u8]) -> String {
    let utf16 = bytes.starts_with(&[0xFF, 0xFE]) || (bytes.len() > 1 && bytes[1] == 0);

    if !utf16 {
        return String::from_utf8_lossy(bytes).into_owned();
    }

    let body = bytes.strip_prefix(&[0xFF, 0xFE]).unwrap_or(bytes);
    let wide: Vec<u16> = body
        .chunks_exact(2)
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .collect();

    String::from_utf16_lossy(&wide)
}

/// Every profile this machine offers, Windows Terminal's and WSL's.
///
/// Reads two files and runs one program, so it is asked for when somebody
/// opens the list rather than on a keystroke. A machine with neither answers
/// with nothing, which is the correct answer rather than an error.
#[cfg(windows)]
pub fn available() -> Vec<Profile> {
    let mut found = settings_file()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .map(|text| profiles_in(&text))
        .unwrap_or_default();

    // A distribution Terminal already has a profile for is not offered twice.
    let known: std::collections::BTreeSet<String> = found
        .iter()
        .map(|one| one.name.to_ascii_lowercase())
        .collect();

    for distribution in installed_distributions() {
        if known.contains(&distribution.to_ascii_lowercase()) {
            continue;
        }

        found.push(Profile {
            name: distribution,
            default: false,
        });
    }

    found
}

#[cfg(not(windows))]
pub fn available() -> Vec<Profile> {
    Vec::new()
}

/// Where Windows Terminal keeps its settings.
///
/// The Store build and the unpackaged build put it in different places, and a
/// machine can have both. First that exists wins, Store first, because that is
/// what `wt.exe` on the PATH resolves to when both are installed.
#[cfg(windows)]
fn settings_file() -> Option<std::path::PathBuf> {
    let local = std::path::PathBuf::from(std::env::var_os("LOCALAPPDATA")?);

    [
        local.join("Packages/Microsoft.WindowsTerminal_8wekyb3d8bbwe/LocalState/settings.json"),
        local.join(
            "Packages/Microsoft.WindowsTerminalPreview_8wekyb3d8bbwe/LocalState/settings.json",
        ),
        local.join("Microsoft/Windows Terminal/settings.json"),
    ]
    .into_iter()
    .find(|one| one.is_file())
}

/// The WSL distributions installed, or nothing if WSL is not here.
#[cfg(windows)]
fn installed_distributions() -> Vec<String> {
    let Ok(out) = std::process::Command::new("wsl.exe")
        .args(["-l", "-q"])
        .output()
    else {
        return Vec::new();
    };

    distributions_in(&console_text(&out.stdout))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The file Terminal itself writes, comments and all.
    const REAL: &str = r#"
{
    // This file was initially generated by Windows Terminal.
    "defaultProfile": "{574e775e-4f2a-5b96-ac1e-a2962a402336}",
    "profiles":
    {
        "list":
        [
            {
                "guid": "{61c54bbd-c2c6-5271-96e7-009a87ff44bf}",
                "name": "Windows PowerShell",
            },
            {
                "guid": "{574e775e-4f2a-5b96-ac1e-a2962a402336}",
                "name": "PowerShell", // the one that ships separately
            },
            {
                "guid": "{2c4de342-38b7-51cf-b940-2309a097f518}",
                "name": "Ubuntu"
            },
            {
                "guid": "{b453ae62-4e3d-5e58-b989-0a998ec441b8}",
                "name": "Azure Cloud Shell",
                "hidden": true
            }
        ]
    }
}
"#;

    #[test]
    fn the_file_terminal_actually_writes_can_be_read() {
        let found = profiles_in(REAL);
        let names: Vec<&str> = found.iter().map(|one| one.name.as_str()).collect();

        assert_eq!(names, ["Windows PowerShell", "PowerShell", "Ubuntu"]);
    }

    /// A profile somebody hid is one they have already refused.
    #[test]
    fn a_hidden_profile_is_not_offered() {
        assert!(!profiles_in(REAL)
            .iter()
            .any(|one| one.name.contains("Azure")));
    }

    #[test]
    fn the_default_profile_is_known_by_its_guid() {
        let found = profiles_in(REAL);
        let default: Vec<&str> = found
            .iter()
            .filter(|one| one.default)
            .map(|one| one.name.as_str())
            .collect();

        assert_eq!(default, ["PowerShell"], "the guid was not matched");
    }

    /// A line comment inside a string is part of the string.
    ///
    /// A strip that worked line by line would eat the rest of this value, and
    /// a `startingDirectory` or an icon URL is exactly where one appears.
    #[test]
    fn a_url_inside_a_value_survives_the_comment_strip() {
        let text = r#"{ "icon": "https://example.com/x.png", "name": "n" }"#;
        assert!(without_comments(text).contains("https://example.com/x.png"));
    }

    #[test]
    fn a_block_comment_goes_too() {
        let text = "{ /* written by the settings UI */ \"a\": 1 }";
        assert_eq!(without_comments(text).replace(' ', ""), "{\"a\":1}");
    }

    #[test]
    fn a_trailing_comma_does_not_stop_the_read() {
        let text = "{ \"a\": [1, 2,], \"b\": 3, }";
        let parsed: serde_json::Value =
            serde_json::from_str(&without_comments(text)).expect("reads");
        assert_eq!(parsed["b"], 3);
    }

    /// A comma inside a string is not a trailing comma.
    #[test]
    fn a_comma_in_a_name_is_left_alone() {
        let text = r#"{ "name": "Ubuntu, again" }"#;
        let parsed: serde_json::Value =
            serde_json::from_str(&without_comments(text)).expect("reads");
        assert_eq!(parsed["name"], "Ubuntu, again");
    }

    #[test]
    fn a_settings_file_that_makes_no_sense_yields_nothing() {
        assert!(profiles_in("this is not json at all").is_empty());
        assert!(profiles_in("").is_empty());
    }

    #[test]
    fn the_older_shape_with_a_bare_list_still_reads() {
        let text = r#"{ "profiles": [ { "name": "Old" } ] }"#;
        let names: Vec<String> = profiles_in(text).into_iter().map(|one| one.name).collect();
        assert_eq!(names, ["Old"]);
    }

    /// Two profiles can share a name, and this machine has a pair.
    ///
    /// `wt -p` takes the first match, so listing both would offer a choice
    /// that cannot be told apart and does not change what happens.
    #[test]
    fn a_name_that_appears_twice_is_offered_once() {
        let text = r#"{ "profiles": { "list": [
            { "guid": "{1}", "name": "Developer Command Prompt for VS 2022" },
            { "guid": "{2}", "name": "Developer Command Prompt for VS 2022" },
            { "guid": "{3}", "name": "Ubuntu" }
        ] } }"#;

        let names: Vec<String> = profiles_in(text).into_iter().map(|one| one.name).collect();
        assert_eq!(names, ["Developer Command Prompt for VS 2022", "Ubuntu"]);
    }

    #[test]
    fn opening_in_a_profile_names_it_before_the_folder() {
        let args = wt_arguments(Some("Ubuntu"), r"C:\work");
        assert_eq!(args, ["-p", "Ubuntu", "-d", r"C:\work"]);
    }

    #[test]
    fn no_profile_is_no_flag_rather_than_an_empty_one() {
        assert_eq!(wt_arguments(None, r"C:\work"), ["-d", r"C:\work"]);
        assert_eq!(wt_arguments(Some("   "), r"C:\work"), ["-d", r"C:\work"]);
    }

    /// A profile name is somebody else's text, and each argument stays its own
    /// argument so nothing has to be quoted by hand.
    #[test]
    fn a_name_with_spaces_or_a_quote_is_one_argument() {
        let args = wt_arguments(Some(r#"Developer "Command" Prompt"#), r"C: b");
        assert_eq!(args.len(), 4);
        assert_eq!(args[1], r#"Developer "Command" Prompt"#);
        assert_eq!(args[3], r"C: b");
    }

    /// The trap that makes WSL look uninstalled.
    #[test]
    fn wsl_answering_in_utf16_is_decoded() {
        let mut bytes = vec![0xFF, 0xFE];
        for unit in "Ubuntu\r\nDebian\r\n".encode_utf16() {
            bytes.extend_from_slice(&unit.to_le_bytes());
        }

        assert_eq!(
            distributions_in(&console_text(&bytes)),
            ["Ubuntu", "Debian"]
        );
    }

    /// And without a byte order mark, which is what actually arrives.
    #[test]
    fn utf16_without_a_mark_is_still_recognised() {
        let mut bytes = Vec::new();
        for unit in "Ubuntu\r\n".encode_utf16() {
            bytes.extend_from_slice(&unit.to_le_bytes());
        }

        assert_eq!(distributions_in(&console_text(&bytes)), ["Ubuntu"]);
    }

    #[test]
    fn ordinary_output_is_not_mangled() {
        assert_eq!(console_text(b"Ubuntu\r\n"), "Ubuntu\r\n");
    }

    #[test]
    fn the_default_marker_and_a_header_are_not_distributions() {
        let listing = "  NAME\r\n* Ubuntu\r\n  Debian\r\n\r\n";
        assert_eq!(distributions_in(listing), ["Ubuntu", "Debian"]);
    }
}
