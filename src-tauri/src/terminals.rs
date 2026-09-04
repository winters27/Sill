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
    /// Whether this is a WSL distribution with no Windows Terminal profile.
    ///
    /// It decides what opening it runs, and it has to: `wt -p Ubuntu` on a
    /// machine whose Terminal has never generated a profile for Ubuntu opens
    /// nothing and says nothing. Those are opened with `wsl.exe -d` instead,
    /// which is what Terminal's generated profile would have run anyway.
    pub distribution: bool,
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
                distribution: false,
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
            distribution: true,
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
///
/// ## Why this stopped running `wsl.exe`
///
/// It used to, and that was fine while the only caller was the settings page,
/// which asks once and waits. It is not fine on a keystroke: **starting
/// `wsl.exe -l -q` on this machine measured 50 to 105 milliseconds**, and a
/// launcher that stops for a tenth of a second because somebody typed a word
/// is the thing rule 7 exists to forbid.
///
/// The registry holds the same list. WSL records every installed distribution
/// under `Lxss`, one key each with its name in a value, which is where
/// `wsl -l` reads it from as well. That is a few microseconds.
///
/// The process is still the fallback, for a WSL old enough not to keep the
/// key, and it only runs when the key is not there at all. So a machine with
/// no WSL pays one failed registry open rather than starting a program to be
/// told the same thing.
#[cfg(windows)]
fn installed_distributions() -> Vec<String> {
    let known = distributions_in_registry();
    if !known.is_empty() {
        return known;
    }

    let Ok(out) = std::process::Command::new("wsl.exe")
        .args(["-l", "-q"])
        .output()
    else {
        return Vec::new();
    };

    distributions_in(&console_text(&out.stdout))
}

/// Where WSL records what is installed.
#[cfg(windows)]
const LXSS: &str = r"Software\Microsoft\Windows\CurrentVersion\Lxss";

/// Every distribution named under `Lxss`, in the order the registry gives.
///
/// One key per distribution, named by a GUID, each holding a `DistributionName`
/// value. A key with no name is one mid-install or mid-removal and is not
/// something to offer.
#[cfg(windows)]
fn distributions_in_registry() -> Vec<String> {
    use windows::core::{HSTRING, PWSTR};
    use windows::Win32::System::Registry::{
        RegCloseKey, RegEnumKeyExW, RegOpenKeyExW, HKEY, HKEY_CURRENT_USER, KEY_READ,
    };

    let mut hive = HKEY::default();

    // SAFETY: the path is a valid wide string and the handle is closed on
    // every way out below.
    let opened = unsafe {
        RegOpenKeyExW(
            HKEY_CURRENT_USER,
            &HSTRING::from(LXSS),
            Some(0),
            KEY_READ,
            &mut hive,
        )
    };

    if opened.is_err() {
        return Vec::new();
    }

    let mut found = Vec::new();
    let mut index = 0u32;

    loop {
        let mut name = [0u16; 256];
        let mut length = name.len() as u32;

        // SAFETY: `length` says how much room `name` has and the call writes
        // no more than that.
        let read = unsafe {
            RegEnumKeyExW(
                hive,
                index,
                Some(PWSTR(name.as_mut_ptr())),
                &mut length,
                None,
                None,
                None,
                None,
            )
        };

        if read.is_err() {
            break;
        }

        let key = String::from_utf16_lossy(&name[..length as usize]);
        if let Some(named) = crate::apps::read_string(
            HKEY_CURRENT_USER,
            &format!(r"{LXSS}\{key}"),
            "DistributionName",
        )
        .filter(|one| !one.trim().is_empty())
        {
            found.push(named);
        }

        index += 1;
    }

    // SAFETY: the handle came from the matching open above.
    unsafe {
        let _ = RegCloseKey(hive);
    }

    found
}

/*
 * The words that ask for a terminal.
 *
 * The first word of the query, exactly, with whatever follows narrowing the
 * list by name. A profile is not an installed application and has no business
 * competing with one for a bare word: "ubuntu" on a machine with the Ubuntu
 * app installed should still find the app.
 *
 * `wsl` lists everything rather than only distributions, deliberately. Windows
 * Terminal generates a profile for each installed distribution, so on nearly
 * every machine the WSL rows and the Terminal rows are the same rows; a filter
 * that told them apart would be sorting by where Sill happened to read the
 * name rather than by anything visible.
 */
const ASKED_BY: &[&str] = &["terminal", "terminals", "wt", "shell", "console", "wsl"];

/// The filter after the word that asked, or nothing if this is not asking.
pub fn asked(query: &str) -> Option<&str> {
    let trimmed = query.trim();
    let (first, rest) = match trimmed.find(char::is_whitespace) {
        Some(at) => (&trimmed[..at], trimmed[at..].trim_start()),
        None => (trimmed, ""),
    };

    if first.is_empty() {
        return None;
    }

    ASKED_BY
        .iter()
        .any(|one| first.eq_ignore_ascii_case(one))
        .then_some(rest)
}

/// The profiles a query asked for, if it asked for any.
///
/// `read` is not called unless the gate opened, which is the whole of the cost
/// claim and is what the counting test proves. The default profile is lifted
/// to the front, because with nothing typed after the word the row somebody
/// means is the one Terminal would have opened anyway.
pub fn matched(query: &str, read: impl FnOnce() -> Vec<Profile>) -> Vec<Profile> {
    let Some(filter) = asked(query) else {
        return Vec::new();
    };

    let wanted = filter.trim().to_lowercase();

    let mut found: Vec<Profile> = read()
        .into_iter()
        .filter(|one| wanted.is_empty() || one.name.to_lowercase().contains(&wanted))
        .collect();

    found.sort_by_key(|one| !one.default);
    found
}

/// What a row carries as its target.
///
/// A row is all the action gets: the settings file and the registry are long
/// behind it by the time somebody presses Enter, and the two kinds of row
/// start two different programs. So the row says which.
///
/// Paired with [`profile_from`], and the pair is round-tripped by a test,
/// because a row built by one and read by the other is exactly the shape of
/// two lists that must agree with nothing making them agree.
pub fn target_of(profile: &Profile) -> &'static str {
    if profile.distribution {
        "wsl"
    } else {
        "wt"
    }
}

/// The profile a row stands for, from what the row carries.
pub fn profile_from(title: &str, target: &str) -> Profile {
    Profile {
        name: title.to_string(),
        // Not carried and not needed: which profile Terminal calls default
        // decides where a row sits in the list, not what opening it does.
        default: false,
        distribution: target == "wsl",
    }
}

/// What to run to open a profile, as a program and its arguments.
///
/// Two programs, and which one is not a detail: `wt -p Ubuntu` opens nothing
/// and says nothing on a machine where Terminal has no profile called Ubuntu,
/// which is exactly the machine where the row came from `Lxss` instead.
///
/// Each argument stays its own argument, for the reason `wt_arguments` says: a
/// profile name is somebody else's text and this machine has one with spaces
/// in it.
pub fn opening(profile: &Profile) -> (&'static str, Vec<String>) {
    if profile.distribution {
        return ("wsl.exe", vec!["-d".to_string(), profile.name.clone()]);
    }

    ("wt.exe", vec!["-p".to_string(), profile.name.clone()])
}

/// How long the list of profiles is reused for.
///
/// A minute, because the answer changes when somebody edits Terminal's
/// settings or installs a distribution, and neither happens while they are
/// typing. Small enough to hold: eight profiles on this machine.
pub const FRESH_FOR: std::time::Duration = std::time::Duration::from_secs(60);

/// The profiles, read at most once every [`FRESH_FOR`].
pub fn now(held: &crate::state::Fresh<Vec<Profile>>) -> Vec<Profile> {
    held.get(available)
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

    fn profile(name: &str, default: bool, distribution: bool) -> Profile {
        Profile {
            name: name.to_string(),
            default,
            distribution,
        }
    }

    /// The whole cost claim for the rows, as a test.
    ///
    /// Not one of these queries may read Terminal's settings file or open a
    /// registry key. "term" and "wsl2" are the ones that matter: both are on
    /// the way to typing something else.
    #[test]
    fn a_query_that_is_not_asking_never_reads_anything() {
        let taken = std::cell::Cell::new(0);
        let read = || {
            taken.set(taken.get() + 1);
            vec![profile("PowerShell", true, false)]
        };

        for query in [
            "",
            "   ",
            "term",
            "termin",
            "terminate",
            "wsl2",
            "shells",
            "consolidate",
            "powershell",
            "ubuntu",
            "chrome",
            "2+2",
        ] {
            assert!(
                matched(query, read).is_empty(),
                "{query:?} produced rows when it is not asking for any"
            );
        }

        assert_eq!(
            taken.get(),
            0,
            "the machine was read {} time(s) for queries that asked nothing",
            taken.get()
        );
    }

    #[test]
    fn the_words_that_ask_read_once_each() {
        for word in ASKED_BY {
            let taken = std::cell::Cell::new(0);
            let read = || {
                taken.set(taken.get() + 1);
                vec![profile("PowerShell", true, false)]
            };

            assert_eq!(matched(word, read).len(), 1, "{word:?} found none");
            assert_eq!(taken.get(), 1, "{word:?} read {} times", taken.get());
        }
    }

    #[test]
    fn the_words_after_it_narrow_the_list_and_the_default_leads_it() {
        let all = || {
            vec![
                profile("Windows PowerShell", false, false),
                profile("Ubuntu", false, true),
                profile("PowerShell", true, false),
            ]
        };

        let names: Vec<String> = matched("terminal", all)
            .into_iter()
            .map(|one| one.name)
            .collect();
        assert_eq!(
            names.first().map(String::as_str),
            Some("PowerShell"),
            "the default profile is not the first row"
        );
        assert_eq!(names.len(), 3);

        let narrowed: Vec<String> = matched("wt ubuntu", all)
            .into_iter()
            .map(|one| one.name)
            .collect();
        assert_eq!(narrowed, ["Ubuntu"]);

        assert!(matched("terminal nothing", all).is_empty());
    }

    /// A row is built by one function and read by another, which is the shape
    /// that has gone wrong here before. This is what makes them agree.
    #[test]
    fn what_a_row_carries_survives_being_read_back() {
        for original in [
            profile("Ubuntu", false, true),
            profile("PowerShell", true, false),
            profile("Developer Command Prompt for VS 2022", false, false),
        ] {
            let read = profile_from(&original.name, target_of(&original));

            assert_eq!(read.name, original.name);
            assert_eq!(
                read.distribution, original.distribution,
                "{} would be opened by the wrong program",
                original.name
            );
            assert_eq!(opening(&read), opening(&original));
        }
    }

    /// The distinction the whole `distribution` flag exists for.
    ///
    /// `wt -p Ubuntu` on a machine whose Terminal has no Ubuntu profile opens
    /// nothing and reports nothing, so a distribution read out of the registry
    /// has to be opened by WSL itself.
    #[test]
    fn a_distribution_is_opened_by_wsl_and_a_profile_by_terminal() {
        assert_eq!(
            opening(&profile("Ubuntu", false, true)),
            ("wsl.exe", vec!["-d".to_string(), "Ubuntu".to_string()])
        );
        assert_eq!(
            opening(&profile("Ubuntu", false, false)),
            ("wt.exe", vec!["-p".to_string(), "Ubuntu".to_string()])
        );
    }

    /// A profile out of the settings file is not a distribution, and one that
    /// was appended because WSL knew about it is.
    #[test]
    fn where_a_profile_came_from_is_recorded() {
        let found = profiles_in(REAL);
        assert!(
            found.iter().all(|one| !one.distribution),
            "a profile out of Terminal's own settings was marked a distribution"
        );
    }

    #[test]
    fn the_default_marker_and_a_header_are_not_distributions() {
        let listing = "  NAME\r\n* Ubuntu\r\n  Debian\r\n\r\n";
        assert_eq!(distributions_in(listing), ["Ubuntu", "Debian"]);
    }
}
