//! What application the user is dictating into.
//!
//! Two things want this. The transcript is filed against it, so the history
//! can say where each dictation went, and it can be fed to the model as part
//! of the biasing prompt: "Visual Studio Code" makes `const`, `async` and
//! `struct` more likely and `constant`, `a sink` and `struck` less so.
//!
//! Read at the moment recording starts, before the panel appears. The panel
//! is declared `focus: false`, so the foreground window is still the one the
//! transcript is headed for.

/// The frontmost application.
///
/// The path travels with the name because they answer different questions:
/// the name is what a person reads, and the path is the only thing an icon
/// can be extracted from. Discarding the path and recovering it from the name
/// later is not possible.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct App {
    pub name: String,
    /// Full path to the executable.
    pub path: String,
}

/// Just the display name, for callers that only prompt with it.
#[cfg(windows)]
pub fn foreground_app() -> Option<String> {
    foreground_app_full().map(|app| app.name)
}

/// The frontmost application, name and path.
///
/// `None` when nothing can be read, which is not an error: dictation works
/// perfectly well without knowing where it is going.
#[cfg(windows)]
pub fn foreground_app_full() -> Option<App> {
    use windows::Win32::Foundation::{CloseHandle, MAX_PATH};
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_FORMAT,
        PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

    // SAFETY: the handle is closed on every path out, and the buffer's length
    // is passed as an in-out parameter exactly as the API requires.
    unsafe {
        let window = GetForegroundWindow();
        if window.is_invalid() {
            return None;
        }

        let mut pid = 0u32;
        GetWindowThreadProcessId(window, Some(&mut pid));
        if pid == 0 {
            return None;
        }

        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;

        let mut buffer = [0u16; MAX_PATH as usize];
        let mut length = buffer.len() as u32;
        let read = QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_FORMAT(0),
            windows::core::PWSTR(buffer.as_mut_ptr()),
            &mut length,
        )
        .is_ok();
        let _ = CloseHandle(handle);

        if !read {
            return None;
        }

        let path = String::from_utf16_lossy(&buffer[..length as usize]);
        Some(App {
            name: tidy(&path),
            path,
        })
    }
}

#[cfg(not(windows))]
pub fn foreground_app() -> Option<String> {
    None
}

#[cfg(not(windows))]
pub fn foreground_app_full() -> Option<App> {
    None
}

/// Turns an executable path into the name a person would use for it.
///
/// Separated out so the naming can be tested without a foreground window,
/// which is the only part with judgement in it.
fn tidy(path: &str) -> String {
    let stem = path
        .rsplit(['\\', '/'])
        .next()
        .unwrap_or(path)
        .trim_end_matches(".exe")
        .trim_end_matches(".EXE");

    if stem.is_empty() {
        return path.to_string();
    }

    // `Code` and `msedge` are what the files are called; nobody says either.
    match stem.to_ascii_lowercase().as_str() {
        "code" => "Visual Studio Code".to_string(),
        "msedge" => "Microsoft Edge".to_string(),
        "windowsterminal" => "Windows Terminal".to_string(),
        "explorer" => "File Explorer".to_string(),
        _ => capitalise(stem),
    }
}

/// Capitalises a name that is entirely lower case.
///
/// Executables are named however their vendor felt: `slack.exe` sits beside
/// `Discord.exe`. A name with any capital in it is left alone, so `OBS` and
/// `WhatsApp` are not flattened on the way past.
fn capitalise(stem: &str) -> String {
    if stem.chars().any(char::is_uppercase) {
        return stem.to_string();
    }

    let mut chars = stem.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

/// The prompt fragment naming `app`, or an empty string when there is none.
///
/// Phrased as a sentence rather than a bare name because the prompt is fed to
/// a speech model as prior text: it conditions on what reads like preceding
/// speech, so a naked token biases less than a sentence containing it.
pub fn prompt_fragment(app: Option<&str>) -> String {
    match app {
        Some(name) if !name.trim().is_empty() => format!("Dictated into {name}."),
        _ => String::new(),
    }
}

/// Joins the prompt pieces, dropping the empty ones.
///
/// Order matters: the vocabulary goes last, closest to what is about to be
/// transcribed, because a speech model conditions most strongly on the tail
/// of its prompt.
pub fn build_prompt(instructions: &str, app: Option<&str>, vocabulary: &str) -> Option<String> {
    let parts = [
        instructions.trim().to_string(),
        prompt_fragment(app),
        vocabulary.trim().to_string(),
    ];

    let joined = parts
        .iter()
        .filter(|part| !part.is_empty())
        .cloned()
        .collect::<Vec<_>>()
        .join(" ");

    (!joined.is_empty()).then_some(joined)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_path_becomes_the_bare_program_name() {
        assert_eq!(tidy(r"C:\Program Files\Slack\slack.exe"), "Slack");
        assert_eq!(tidy("/usr/bin/Foo"), "Foo");
    }

    #[test]
    fn the_names_nobody_actually_says_are_translated() {
        assert_eq!(
            tidy(r"C:\Users\x\AppData\Local\Programs\Microsoft VS Code\Code.exe"),
            "Visual Studio Code"
        );
        assert_eq!(tidy(r"C:\Windows\explorer.exe"), "File Explorer");
    }

    #[test]
    fn a_lower_case_executable_name_is_capitalised() {
        // Vendors are inconsistent, and this name goes into a prompt and
        // onto a history row where "slack" reads as a mistake.
        assert_eq!(tidy(r"C:\x\notepad.exe"), "Notepad");
    }

    #[test]
    fn a_name_that_already_has_capitals_is_left_alone() {
        assert_eq!(tidy(r"C:\x\WhatsApp.exe"), "WhatsApp");
        assert_eq!(tidy(r"C:\x\OBS.exe"), "OBS");
    }

    #[test]
    fn an_unreadable_path_is_returned_rather_than_becoming_empty() {
        assert_eq!(tidy(""), "");
        assert_eq!(tidy(".exe"), ".exe", "a name that is only a suffix is kept");
    }

    #[test]
    fn no_app_contributes_nothing_to_the_prompt() {
        assert_eq!(prompt_fragment(None), "");
        assert_eq!(prompt_fragment(Some("  ")), "");
    }

    #[test]
    fn the_prompt_puts_the_vocabulary_last() {
        // A speech model conditions most strongly on the tail of its prompt,
        // so the names that must come out right go closest to the speech.
        let prompt = build_prompt(
            "Prefer British spelling.",
            Some("Slack"),
            "Vicinae, Raycast",
        )
        .expect("all three parts present");

        assert!(prompt.ends_with("Vicinae, Raycast"), "got {prompt}");
        assert!(prompt.starts_with("Prefer British spelling."));
        assert!(prompt.contains("Dictated into Slack."));
    }

    #[test]
    fn an_empty_part_is_dropped_rather_than_leaving_a_gap() {
        assert_eq!(
            build_prompt("", Some("Slack"), "").as_deref(),
            Some("Dictated into Slack.")
        );
        assert_eq!(build_prompt("   ", None, "  "), None);
    }
}
