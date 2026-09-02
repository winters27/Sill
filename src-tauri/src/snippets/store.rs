//! The snippets themselves.
//!
//! A JSON file rather than SQLite: a person has tens of snippets, not tens of
//! thousands, and a file they can open, read and edit by hand is worth more
//! here than an index. The clipboard history made the opposite call for the
//! opposite reason.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

/// One snippet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Snippet {
    /// Stable across renames, because the frecency and the editor both refer
    /// to it and a name is the thing most likely to change.
    pub id: String,
    pub name: String,
    /// Typed anywhere to expand it. Empty means it is only reachable from the
    /// launcher, which is a legitimate way to keep one.
    pub keyword: String,
    /// The text, with placeholders still in it.
    pub content: String,
    pub uses: u64,
    /// Unix seconds.
    pub created: i64,
    /// Only fire when the keyword stands as a whole word.
    ///
    /// Without this a keyword of `sig` fires inside "design", which is why
    /// everyone's snippets end up with a punctuation prefix like `;sig`.
    /// Every mature expander offers it; espanso calls it `word`.
    pub whole_word: bool,
    /// The group it belongs to. Empty means it belongs to no group.
    ///
    /// A name rather than an id, because a collection is nothing but a name:
    /// there is no list of them anywhere, they exist because snippets say
    /// they do, and renaming one is renaming it on the snippets that carry it.
    /// A separate table would be a second place for them to disagree.
    pub collection: String,
    /// The programs it may expand in. Empty means anywhere.
    ///
    /// Matched against the foreground program's file name without its
    /// extension, so "code" covers `C:\...\Code.exe` wherever it is
    /// installed. Case is ignored, because Windows ignores it.
    pub only_in: Vec<String>,
    /// The same content with its formatting, when it has any.
    ///
    /// Empty for the great majority. Kept **beside** `content` rather than
    /// instead of it: the plain text is what a plain field receives, what the
    /// launcher shows as a preview, and what is left if the markup is ever
    /// unreadable. Two representations of one thing, and the plain one is the
    /// one that always works.
    pub html: String,
}

impl Default for Snippet {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            keyword: String::new(),
            content: String::new(),
            uses: 0,
            created: 0,
            // On by default, because it is what someone typing `sig` in the
            // middle of "design" expects, and the surprising direction to
            // fail in is firing when you did not mean it.
            whole_word: true,
            collection: String::new(),
            only_in: Vec::new(),
            html: String::new(),
        }
    }
}

pub fn path(app: &AppHandle) -> PathBuf {
    app.path()
        .app_data_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("snippets.json")
}

/// Reads the file, or an empty list when there is not one yet.
///
/// A malformed file yields nothing rather than an error: a launcher that
/// refuses to start because one snippet is unparseable is worse than one that
/// is briefly missing its snippets, and the file is right there to fix.
pub fn load(file: &Path) -> Vec<Snippet> {
    let Ok(text) = std::fs::read_to_string(file) else {
        return Vec::new();
    };

    match serde_json::from_str(&text) {
        Ok(snippets) => snippets,
        Err(err) => {
            crate::say!("snippets could not be read: {err}");
            Vec::new()
        }
    }
}

/// Writes the whole list.
///
/// Staged and renamed, so an interrupted write cannot leave a truncated file
/// where every snippet used to be.
pub fn save(file: &Path, snippets: &[Snippet]) -> std::io::Result<()> {
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let body = serde_json::to_string_pretty(snippets).unwrap_or_else(|_| "[]".to_string());

    let staging = file.with_extension("json.partial");
    std::fs::write(&staging, body)?;
    if let Err(err) = std::fs::rename(&staging, file) {
        let _ = std::fs::remove_file(&staging);
        return Err(err);
    }
    Ok(())
}

/// Adds or replaces one snippet, keyed by id.
pub fn upsert(snippets: &mut Vec<Snippet>, snippet: Snippet) {
    match snippets.iter_mut().find(|s| s.id == snippet.id) {
        Some(existing) => {
            // The count belongs to the snippet, not to the edit that saved
            // it, so editing a snippet must not reset how often it is used.
            let uses = existing.uses;
            let created = existing.created;
            *existing = snippet;
            existing.uses = uses;
            existing.created = created;
        }
        None => snippets.push(snippet),
    }
}

/// The snippet whose keyword the typed text ends with, if any.
///
/// Matches on the **end** of the buffer rather than the whole of it, because
/// a keyword is typed in the middle of a sentence. The longest match wins, so
/// `addr` and `addrwork` can both exist and the longer one is not shadowed by
/// the shorter one firing first.
pub fn match_keyword<'a>(
    snippets: &'a [Snippet],
    typed: &str,
    /*
     * Which program is in front, asked for **only if it turns out to matter**.
     *
     * This runs inside a low-level keyboard hook, on every character typed
     * anywhere on the machine, and reading the foreground program is a window
     * handle, a process handle and a path. Paying that per keystroke to serve
     * the rare snippet that is limited to one program would be the whole
     * feature's cost falling on everybody who does not use it.
     *
     * So: match on the text first, and consult this only when the snippet that
     * won is limited. Nearly always it is never called at all.
     */
    foreground: impl FnOnce() -> Option<String>,
) -> Option<&'a Snippet> {
    let mut candidates: Vec<&Snippet> = snippets
        .iter()
        .filter(|snippet| {
            let keyword = snippet.keyword.trim();
            if keyword.is_empty() || !typed.ends_with(keyword) {
                return false;
            }
            !snippet.whole_word || starts_a_word(typed, keyword)
        })
        .collect();

    // Longest keyword first, which is what the shorter form used to do with
    // `max_by_key`. Two snippets ending the same way means the more specific
    // one was meant.
    candidates.sort_by_key(|snippet| std::cmp::Reverse(snippet.keyword.trim().chars().count()));

    // The common case, and the one that must not pay for the other.
    if candidates
        .first()
        .is_some_and(|best| best.only_in.is_empty())
    {
        return candidates.into_iter().next();
    }

    if candidates.is_empty() {
        return None;
    }

    let here = foreground();

    candidates
        .into_iter()
        .find(|snippet| allowed_in(snippet, here.as_deref()))
}

/// Whether this snippet may expand in the program named.
///
/// Matched on the program's own name without its extension, so "code" covers
/// that editor wherever it happens to be installed and whatever the folder is
/// called. Case is ignored, because Windows ignores it.
///
/// A snippet limited to somewhere does **not** expand when the program cannot
/// be read at all. The safe direction is not firing: a signature appearing in
/// the wrong window is worse than one that has to be typed.
pub fn allowed_in(snippet: &Snippet, program: Option<&str>) -> bool {
    if snippet.only_in.is_empty() {
        return true;
    }

    let Some(program) = program else {
        return false;
    };

    let program = std::path::Path::new(program)
        .file_stem()
        .map(|stem| stem.to_string_lossy().to_string())
        .unwrap_or_else(|| program.to_string());

    snippet.only_in.iter().any(|wanted| {
        let wanted = std::path::Path::new(wanted.trim())
            .file_stem()
            .map(|stem| stem.to_string_lossy().to_string())
            .unwrap_or_default();

        !wanted.is_empty() && wanted.eq_ignore_ascii_case(&program)
    })
}

/// Past this many characters, typing is abandoned for a paste.
///
/// `SendInput` costs two records per character, so a 600-character
/// snippet is 1,200 events. That is visibly slow to watch appear, and
/// several classes of application (Electron, terminals, remote desktop)
/// drop synthetic input arriving that fast. Every mature expander has the
/// same threshold for the same reason.
///
/// Below it, typing is strictly better: it leaves the clipboard alone.
pub const TYPE_LIMIT: usize = 200;

/// Whether this expansion goes through the clipboard rather than the keyboard.
///
/// Its own function so the rule can be stated once and checked: **formatting
/// only travels through the clipboard.** There is no way to type bold, so a
/// snippet that has any is pasted however short it is, and one that has none
/// is typed unless it is long.
pub fn wants_pasting(text: &str, html: &str) -> bool {
    !html.trim().is_empty() || text.chars().count() > TYPE_LIMIT
}

/// Whether the keyword at the end of `typed` begins a word.
///
/// A keyword that itself starts with punctuation is **self-delimiting**: the
/// `;` in `;sig` is the boundary, so what precedes it is irrelevant and
/// `hello;sig` must still fire. Only a keyword starting with a word character
/// needs the character before it to be a boundary.
fn starts_a_word(typed: &str, keyword: &str) -> bool {
    if !keyword.chars().next().is_some_and(char::is_alphanumeric) {
        return true;
    }

    let before = typed.len() - keyword.len();
    match typed[..before].chars().next_back() {
        None => true,
        Some(c) => !c.is_alphanumeric(),
    }
}

/// Whether `keyword` can be used by `id` without clashing.
///
/// Two snippets sharing a keyword means one of them can never fire, which is
/// a confusing thing to discover later rather than at the moment of saving.
pub fn keyword_is_free(snippets: &[Snippet], id: &str, keyword: &str) -> bool {
    let keyword = keyword.trim();
    if keyword.is_empty() {
        return true;
    }

    !snippets
        .iter()
        .any(|s| s.id != id && s.keyword.trim().eq_ignore_ascii_case(keyword))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Formatting only travels through the clipboard.
    mod through_the_clipboard_or_the_keyboard {
        use super::*;

        /// There is no way to type bold, so any formatting means a paste
        /// however short the snippet is.
        #[test]
        fn anything_formatted_is_pasted() {
            assert!(wants_pasting("hi", "<b>hi</b>"));
        }

        /// Typing leaves the clipboard alone, which is the whole reason
        /// snippets are typed at all.
        #[test]
        fn short_plain_text_is_typed() {
            assert!(!wants_pasting("Kind regards", ""));
        }

        #[test]
        fn long_plain_text_is_pasted() {
            assert!(wants_pasting(&"x".repeat(TYPE_LIMIT + 1), ""));
            assert!(!wants_pasting(&"x".repeat(TYPE_LIMIT), ""));
        }

        /// Markup that is only whitespace is not markup, and pasting for it
        /// would borrow the clipboard for nothing.
        #[test]
        fn empty_markup_does_not_count_as_formatting() {
            assert!(!wants_pasting("hi", "   "));
        }
    }

    /// A snippet can be limited to the programs it makes sense in.
    ///
    /// A signature belongs in mail and a code fragment belongs in an editor,
    /// and a keyword short enough to be worth typing is short enough to fire
    /// somewhere it should not.
    mod only_in_certain_programs {
        use super::*;

        fn limited(keyword: &str, to: &[&str]) -> Snippet {
            Snippet {
                id: "s".into(),
                name: "One".into(),
                keyword: keyword.into(),
                content: "x".into(),
                only_in: to.iter().map(|program| (*program).to_string()).collect(),
                ..Default::default()
            }
        }

        #[test]
        fn it_fires_in_a_program_it_names() {
            let snippets = vec![limited(";sig", &["outlook"])];

            assert!(match_keyword(&snippets, ";sig", || {
                Some(r"C:\Program Files\Office\OUTLOOK.EXE".to_string())
            })
            .is_some());
        }

        #[test]
        fn it_does_not_fire_anywhere_else() {
            let snippets = vec![limited(";sig", &["outlook"])];

            assert!(match_keyword(&snippets, ";sig", || {
                Some(r"C:\Windows\notepad.exe".to_string())
            })
            .is_none());
        }

        /// The safe direction is not firing. A signature appearing in the
        /// wrong window is worse than one that has to be typed.
        #[test]
        fn it_does_not_fire_when_the_program_cannot_be_read() {
            let snippets = vec![limited(";sig", &["outlook"])];

            assert!(match_keyword(&snippets, ";sig", || None).is_none());
        }

        /// Matched on the program's own name, so where it is installed and
        /// what the folder is called do not come into it.
        #[test]
        fn the_path_and_the_casing_do_not_matter() {
            let snippets = vec![limited(";sig", &["Code.exe"])];

            for program in [
                r"C:\Users\x\AppData\Local\Programs\Microsoft VS Code\Code.exe",
                r"D:\portable\code.EXE",
                "code",
            ] {
                assert!(
                    match_keyword(&snippets, ";sig", || Some(program.to_string())).is_some(),
                    "{program} did not match",
                );
            }
        }

        /// The whole point of the laziness. This runs inside a keyboard hook
        /// on every character typed anywhere, and reading the foreground
        /// program is three system calls: the snippet nobody limited must not
        /// pay for the one somebody did.
        #[test]
        fn nothing_is_asked_about_the_foreground_unless_it_matters() {
            let plain = vec![Snippet {
                id: "s".into(),
                keyword: ";sig".into(),
                content: "x".into(),
                ..Default::default()
            }];

            let mut asked = false;
            let found = match_keyword(&plain, ";sig", || {
                asked = true;
                None
            });

            assert!(found.is_some());
            assert!(
                !asked,
                "the foreground program was read for an unlimited snippet"
            );
        }

        /// Nothing is asked when nothing matched either, which is every
        /// keystroke that is not the end of a keyword.
        #[test]
        fn nothing_is_asked_when_nothing_matched() {
            let snippets = vec![limited(";sig", &["outlook"])];

            let mut asked = false;
            let found = match_keyword(&snippets, "just typing", || {
                asked = true;
                None
            });

            assert!(found.is_none());
            assert!(!asked);
        }

        /// A limited snippet losing does not take an unlimited one with it.
        #[test]
        fn a_snippet_for_anywhere_still_fires_when_a_limited_one_cannot() {
            let snippets = vec![
                limited(";sig", &["outlook"]),
                Snippet {
                    id: "other".into(),
                    keyword: ";sig".into(),
                    content: "anywhere".into(),
                    ..Default::default()
                },
            ];

            let found = match_keyword(&snippets, ";sig", || {
                Some(r"C:\Windows\notepad.exe".to_string())
            });

            assert_eq!(found.map(|snippet| snippet.id.as_str()), Some("other"));
        }
    }

    /// `match_keyword` with nothing in front, which is what every test that
    /// is not about the program restriction wants.
    fn matched<'a>(snippets: &'a [Snippet], typed: &str) -> Option<&'a Snippet> {
        match_keyword(snippets, typed, || None)
    }

    fn snippet(id: &str, keyword: &str) -> Snippet {
        Snippet {
            id: id.into(),
            name: id.into(),
            keyword: keyword.into(),
            content: format!("content of {id}"),
            ..Default::default()
        }
    }

    #[test]
    fn a_keyword_matches_at_the_end_of_what_was_typed() {
        // Keywords are typed mid-sentence, so the whole buffer will never
        // equal one.
        let snippets = vec![snippet("a", ";sig")];
        assert!(matched(&snippets, "hello there ;sig").is_some());
        assert!(matched(&snippets, ";sig").is_some());
        assert!(matched(&snippets, ";sig and more").is_none());
    }

    #[test]
    fn the_longest_keyword_wins() {
        // Otherwise `addr` fires first and `addrwork` can never be typed.
        let snippets = vec![
            Snippet {
                whole_word: false,
                ..snippet("short", "addr")
            },
            Snippet {
                whole_word: false,
                ..snippet("long", "addrwork")
            },
        ];
        assert_eq!(matched(&snippets, "my addrwork").unwrap().id, "long");
        assert_eq!(matched(&snippets, "my addr").unwrap().id, "short");
    }

    #[test]
    fn a_whole_word_keyword_does_not_fire_inside_another_word() {
        // The reason everyone's snippets end up prefixed with punctuation.
        let snippets = vec![snippet("a", "sig")];

        assert!(matched(&snippets, "sig").is_some());
        assert!(matched(&snippets, "my sig").is_some());
        assert!(
            matched(&snippets, "(sig").is_some(),
            "punctuation is a boundary"
        );
        assert!(matched(&snippets, "resig").is_none());
        assert!(
            matched(&snippets, "1sig").is_none(),
            "a digit is a word character"
        );
    }

    #[test]
    fn turning_off_whole_word_lets_it_fire_anywhere() {
        let snippets = vec![Snippet {
            whole_word: false,
            ..snippet("a", "sig")
        }];
        assert!(matched(&snippets, "resig").is_some());
    }

    #[test]
    fn a_punctuation_prefixed_keyword_works_either_way() {
        // The `;sig` convention is a boundary by definition, so whole-word
        // matching must not break the habit people already have.
        let snippets = vec![snippet("a", ";sig")];
        assert!(matched(&snippets, "hello;sig").is_some());
        assert!(matched(&snippets, "hello ;sig").is_some());
    }

    #[test]
    fn a_snippet_with_no_keyword_never_fires() {
        // It is still reachable from the launcher, which is a legitimate way
        // to keep one, but it must not match every keystroke.
        let snippets = vec![snippet("a", ""), snippet("b", "   ")];
        assert!(matched(&snippets, "anything at all").is_none());
        assert!(matched(&snippets, "").is_none());
    }

    #[test]
    fn editing_a_snippet_keeps_its_history() {
        // The count belongs to the snippet, not to the edit that saved it.
        let mut snippets = vec![Snippet {
            uses: 12,
            created: 500,
            ..snippet("a", ";x")
        }];

        upsert(&mut snippets, snippet("a", ";y"));

        assert_eq!(snippets.len(), 1, "the same id replaces rather than adds");
        assert_eq!(snippets[0].keyword, ";y");
        assert_eq!(snippets[0].uses, 12);
        assert_eq!(snippets[0].created, 500);
    }

    #[test]
    fn a_new_id_is_added_rather_than_replacing() {
        let mut snippets = vec![snippet("a", ";a")];
        upsert(&mut snippets, snippet("b", ";b"));
        assert_eq!(snippets.len(), 2);
    }

    #[test]
    fn a_clashing_keyword_is_refused_before_it_is_saved() {
        // Two snippets sharing a keyword means one can never fire, which is
        // confusing to discover later rather than at the moment of saving.
        let snippets = vec![snippet("a", ";sig")];

        assert!(!keyword_is_free(&snippets, "b", ";sig"));
        assert!(
            !keyword_is_free(&snippets, "b", ";SIG"),
            "case does not rescue it"
        );
        assert!(
            keyword_is_free(&snippets, "a", ";sig"),
            "its own keyword is fine"
        );
        assert!(keyword_is_free(&snippets, "b", ";other"));
        assert!(
            keyword_is_free(&snippets, "b", ""),
            "no keyword clashes with nothing"
        );
    }

    #[test]
    fn a_round_trip_through_the_file_preserves_everything() {
        let dir = tempfile::tempdir().expect("temp dir");
        let file = dir.path().join("snippets.json");

        let original = vec![Snippet {
            uses: 3,
            created: 1_700_000_000,
            content: "Hi {cursor},\n\nBest".into(),
            ..snippet("a", ";sig")
        }];
        save(&file, &original).expect("saves");

        let back = load(&file);
        assert_eq!(back.len(), 1);
        assert_eq!(back[0].content, "Hi {cursor},\n\nBest");
        assert_eq!(back[0].uses, 3);
    }

    #[test]
    fn a_missing_or_broken_file_yields_nothing_rather_than_failing() {
        // A launcher that will not start because one snippet is unparseable
        // is worse than one briefly missing its snippets.
        let dir = tempfile::tempdir().expect("temp dir");
        assert!(load(&dir.path().join("absent.json")).is_empty());

        let broken = dir.path().join("broken.json");
        std::fs::write(&broken, "{ not json").expect("writes");
        assert!(load(&broken).is_empty());
    }

    #[test]
    fn saving_leaves_no_staging_file_behind() {
        let dir = tempfile::tempdir().expect("temp dir");
        let file = dir.path().join("snippets.json");
        save(&file, &[snippet("a", ";a")]).expect("saves");

        assert!(file.is_file());
        assert!(!file.with_extension("json.partial").exists());
    }
}
