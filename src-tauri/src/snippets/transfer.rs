//! Getting snippets in and out of a file.
//!
//! Snippets are the one thing in a launcher people have already written
//! somewhere else. Somebody arriving with fifty of them in another tool is not
//! going to retype them, and somebody leaving should not have to feel trapped
//! to stay.
//!
//! The reading is deliberately forgiving and the merging deliberately is not.
//! A file can be written by anything and should be read as generously as it
//! can be understood; what happens to somebody's existing snippets afterwards
//! should be exactly one predictable thing.

use serde::Deserialize;

use super::store::Snippet;

/// One snippet as a file describes it, before anything is decided about it.
///
/// Every field optional, because this is what a file said rather than what
/// Sill holds. A file missing the text is the only one that cannot be used.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Incoming {
    id: Option<String>,
    /// What Sill calls it.
    name: Option<String>,
    /// What another tool might call the same thing.
    title: Option<String>,
    /// What Sill calls the body.
    content: Option<String>,
    /// What other tools call the body. Both spellings are common enough to be
    /// worth reading, and neither is ambiguous.
    text: Option<String>,
    snippet: Option<String>,
    keyword: Option<String>,
    /// Another spelling of the same idea.
    shortcut: Option<String>,
    whole_word: Option<bool>,
}

impl Incoming {
    /// The body, whichever way the file spelled it.
    fn body(&self) -> Option<&str> {
        self.content
            .as_deref()
            .or(self.text.as_deref())
            .or(self.snippet.as_deref())
            .map(str::trim_end)
            .filter(|body| !body.is_empty())
    }

    fn label(&self) -> String {
        let named = self
            .name
            .as_deref()
            .or(self.title.as_deref())
            .map(str::trim)
            .filter(|name| !name.is_empty());

        match named {
            Some(name) => name.to_string(),
            // A file with no name still describes a usable snippet, and a list
            // row with nothing on it is worse than a first line.
            None => first_line(self.body().unwrap_or_default()),
        }
    }

    fn trigger(&self) -> String {
        self.keyword
            .as_deref()
            .or(self.shortcut.as_deref())
            .unwrap_or_default()
            .trim()
            .to_string()
    }
}

/// The opening of a snippet, for naming one that arrived without a name.
fn first_line(body: &str) -> String {
    let line = body.lines().next().unwrap_or_default().trim();

    if line.chars().count() <= 40 {
        return line.to_string();
    }

    let short: String = line.chars().take(39).collect();
    format!("{short}\u{2026}")
}

/// What an import did, so it can be said out loud rather than guessed at.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub added: usize,
    /// Already here under the same id, and replaced.
    pub updated: usize,
    /// Already here word for word, and left alone.
    pub skipped: usize,
    /// Arrived with a keyword another snippet already uses.
    ///
    /// Imported without it rather than dropped or allowed to collide: the text
    /// is the valuable part, and two snippets answering one keyword is a
    /// silent coin toss every time it is typed.
    pub keywords_taken: usize,
}

/// Reads a file of snippets, however it was written.
///
/// Accepts a bare array or an object with the array under `snippets`, and
/// reads either spelling of a name, a body and a keyword. Anything with no
/// body at all is skipped, since there is nothing to expand.
pub fn parse(text: &str) -> Result<Vec<Snippet>, String> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum File {
        Bare(Vec<Incoming>),
        Wrapped { snippets: Vec<Incoming> },
    }

    let parsed: File = serde_json::from_str(text)
        .map_err(|err| format!("that file is not snippets Sill can read: {err}"))?;

    let rows = match parsed {
        File::Bare(rows) => rows,
        File::Wrapped { snippets } => snippets,
    };

    Ok(rows
        .into_iter()
        .filter_map(|row| {
            let content = row.body()?.to_string();

            Some(Snippet {
                id: row.id.clone().unwrap_or_default(),
                name: row.label(),
                keyword: row.trigger(),
                content,
                uses: 0,
                created: 0,
                whole_word: row.whole_word.unwrap_or(true),
            })
        })
        .collect())
}

/// Folds imported snippets into the ones already here.
///
/// Pure, and the whole of the policy. Importing is the one operation that can
/// quietly ruin a collection somebody has built up, so what it does is decided
/// in one place with nothing else going on.
pub fn merge(existing: &[Snippet], incoming: Vec<Snippet>, now: i64) -> (Vec<Snippet>, Summary) {
    let mut out = existing.to_vec();
    let mut summary = Summary::default();

    for mut arriving in incoming {
        // Already here under the same id: the same snippet, imported again.
        // Replaced rather than duplicated, and its history is kept, because
        // how often somebody reached for it is a fact about them rather than
        // about the file.
        if !arriving.id.is_empty() {
            if let Some(at) = out.iter().position(|held| held.id == arriving.id) {
                let taken = keyword_taken(&out, &arriving.id, &arriving.keyword);
                if taken {
                    arriving.keyword = String::new();
                    summary.keywords_taken += 1;
                }

                arriving.uses = out[at].uses;
                arriving.created = out[at].created;
                out[at] = arriving;
                summary.updated += 1;
                continue;
            }
        }

        // Word for word what is already here. Importing the same file twice
        // should not leave two of everything.
        if out
            .iter()
            .any(|held| held.name == arriving.name && held.content == arriving.content)
        {
            summary.skipped += 1;
            continue;
        }

        if arriving.id.is_empty() {
            arriving.id = super::commands::new_id();
        }

        if keyword_taken(&out, &arriving.id, &arriving.keyword) {
            arriving.keyword = String::new();
            summary.keywords_taken += 1;
        }

        arriving.created = now;
        out.push(arriving);
        summary.added += 1;
    }

    (out, summary)
}

/// Whether some other snippet already answers to this keyword.
fn keyword_taken(existing: &[Snippet], id: &str, keyword: &str) -> bool {
    if keyword.is_empty() {
        return false;
    }

    existing
        .iter()
        .any(|held| held.id != id && held.keyword.eq_ignore_ascii_case(keyword))
}

/// Writes snippets as a file another tool has a chance of reading.
///
/// The shape Sill holds, plus `text` beside `content` saying the same thing.
/// Costing a few bytes to be readable by anything that expects the commoner
/// spelling is a better trade than being a format only one program knows.
pub fn to_json(snippets: &[Snippet]) -> String {
    let rows: Vec<serde_json::Value> = snippets
        .iter()
        .map(|snippet| {
            serde_json::json!({
                "id": snippet.id,
                "name": snippet.name,
                "keyword": snippet.keyword,
                "content": snippet.content,
                "text": snippet.content,
                "wholeWord": snippet.whole_word,
            })
        })
        .collect();

    serde_json::to_string_pretty(&rows).unwrap_or_else(|_| "[]".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snippet(id: &str, name: &str, keyword: &str, content: &str) -> Snippet {
        Snippet {
            id: id.into(),
            name: name.into(),
            keyword: keyword.into(),
            content: content.into(),
            uses: 0,
            created: 1_700_000_000,
            whole_word: true,
        }
    }

    // ------------------------------------------------------------- reading

    #[test]
    fn a_file_from_another_tool_reads() {
        // The shape those tools write: a bare array of name, keyword and text.
        let found = parse(
            r#"[
                {"name": "Signature", "keyword": ";sig", "text": "Best,\nBrandon"},
                {"name": "Address", "keyword": ";addr", "text": "12 Somewhere"}
            ]"#,
        )
        .expect("reads");

        assert_eq!(found.len(), 2);
        assert_eq!(found[0].name, "Signature");
        assert_eq!(found[0].keyword, ";sig");
        assert_eq!(found[0].content, "Best,\nBrandon");
    }

    #[test]
    fn a_file_sill_wrote_reads_back() {
        let mine = to_json(&[snippet("abc", "Signature", ";sig", "Best,\nBrandon")]);
        let found = parse(&mine).expect("reads");

        assert_eq!(found.len(), 1);
        assert_eq!(found[0].id, "abc");
        assert_eq!(found[0].name, "Signature");
        assert_eq!(found[0].content, "Best,\nBrandon");
    }

    #[test]
    fn either_spelling_of_a_field_is_understood() {
        // A file can be written by anything, and reading it generously costs
        // nothing where the alternatives are unambiguous.
        let found = parse(
            r#"[
                {"title": "One", "snippet": "first", "shortcut": ";a"},
                {"name": "Two", "content": "second", "keyword": ";b"}
            ]"#,
        )
        .expect("reads");

        assert_eq!(found[0].name, "One");
        assert_eq!(found[0].content, "first");
        assert_eq!(found[0].keyword, ";a");
        assert_eq!(found[1].name, "Two");
    }

    #[test]
    fn an_array_wrapped_in_an_object_reads_too() {
        let found = parse(r#"{"snippets": [{"name": "One", "text": "first"}]}"#).expect("reads");

        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "One");
    }

    #[test]
    fn a_snippet_with_no_body_is_skipped_rather_than_imported_empty() {
        // There is nothing to expand. An empty one in the list is a row that
        // does nothing and cannot be told apart from a mistake.
        let found = parse(
            r#"[
                {"name": "Nothing", "keyword": ";x"},
                {"name": "Blank", "text": "   ", "keyword": ";y"},
                {"name": "Real", "text": "something"}
            ]"#,
        )
        .expect("reads");

        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "Real");
    }

    #[test]
    fn a_snippet_with_no_name_is_named_after_its_first_line() {
        // Better than an empty row. A file that only carried keywords and text
        // still produces a list somebody can read.
        let found = parse(r#"[{"text": "Best,\nBrandon", "keyword": ";sig"}]"#).expect("reads");

        assert_eq!(found[0].name, "Best,");
    }

    #[test]
    fn a_very_long_first_line_is_cut_rather_than_used_whole() {
        let long = "x".repeat(200);
        let found = parse(&format!(r#"[{{"text": "{long}"}}]"#)).expect("reads");

        assert_eq!(found[0].name.chars().count(), 40, "{}", found[0].name);
        assert!(found[0].name.ends_with('\u{2026}'));
    }

    #[test]
    fn something_that_is_not_snippets_is_refused_by_name() {
        // Rather than silently importing nothing, which reads as the file
        // being empty rather than as the wrong file being chosen.
        assert!(parse("not json at all").is_err());
        assert!(parse(r#"{"preferences": {"theme": "dark"}}"#).is_err());
    }

    // ------------------------------------------------------------- merging

    const NOW: i64 = 1_800_000_000;

    #[test]
    fn importing_adds_what_is_new() {
        let held = vec![snippet("a", "One", ";a", "first")];
        let arriving = parse(r#"[{"name": "Two", "text": "second", "keyword": ";b"}]"#).unwrap();

        let (out, summary) = merge(&held, arriving, NOW);

        assert_eq!(out.len(), 2);
        assert_eq!(summary.added, 1);
        assert_eq!(out[1].created, NOW, "arrived without a date of its own");
        assert!(!out[1].id.is_empty(), "arrived without an id");
    }

    #[test]
    fn importing_the_same_file_twice_does_not_leave_two_of_everything() {
        // The commonest way to try an import: do it, then do it again because
        // it was not obvious the first one worked.
        let arriving = parse(r#"[{"name": "One", "text": "first", "keyword": ";a"}]"#).unwrap();
        let (once, _) = merge(&[], arriving.clone(), NOW);
        let (twice, summary) = merge(&once, arriving, NOW);

        assert_eq!(twice.len(), 1, "{twice:?}");
        assert_eq!(summary.skipped, 1);
        assert_eq!(summary.added, 0);
    }

    #[test]
    fn a_snippet_that_is_already_here_by_id_is_replaced_and_keeps_its_history() {
        // How often somebody reached for it is a fact about them, not about
        // the file, and an export that has been sitting around for a month
        // should not reset it.
        let held = vec![Snippet {
            uses: 42,
            ..snippet("a", "One", ";a", "old text")
        }];
        let arriving = parse(r#"[{"id": "a", "name": "One", "text": "new text"}]"#).unwrap();

        let (out, summary) = merge(&held, arriving, NOW);

        assert_eq!(out.len(), 1);
        assert_eq!(out[0].content, "new text");
        assert_eq!(out[0].uses, 42, "history was thrown away");
        assert_eq!(out[0].created, 1_700_000_000, "the original date was lost");
        assert_eq!(summary.updated, 1);
    }

    #[test]
    fn a_keyword_another_snippet_already_uses_is_dropped_and_counted() {
        // Two snippets answering one keyword is a silent coin toss every time
        // it is typed. The text is the valuable part, so it arrives without
        // the keyword and the count says so out loud.
        let held = vec![snippet("a", "Mine", ";sig", "mine")];
        let arriving = parse(r#"[{"name": "Theirs", "text": "theirs", "keyword": ";sig"}]"#).unwrap();

        let (out, summary) = merge(&held, arriving, NOW);

        assert_eq!(out.len(), 2);
        assert_eq!(out[0].keyword, ";sig", "the one already here lost its keyword");
        assert_eq!(out[1].keyword, "", "the arriving one kept a taken keyword");
        assert_eq!(summary.keywords_taken, 1);
        assert_eq!(summary.added, 1);
    }

    #[test]
    fn a_keyword_is_taken_regardless_of_how_it_was_capitalised() {
        // Expansion does not care about case, so neither can this. Letting
        // `;SIG` in beside `;sig` produces exactly the collision the check
        // exists to prevent.
        let held = vec![snippet("a", "Mine", ";sig", "mine")];
        let arriving = parse(r#"[{"name": "Theirs", "text": "theirs", "keyword": ";SIG"}]"#).unwrap();

        let (_, summary) = merge(&held, arriving, NOW);

        assert_eq!(summary.keywords_taken, 1);
    }

    #[test]
    fn a_snippet_keeps_its_own_keyword_when_it_is_replacing_itself() {
        // Its keyword is taken by itself, which must not read as a collision.
        let held = vec![snippet("a", "One", ";a", "old")];
        let arriving = parse(r#"[{"id": "a", "name": "One", "text": "new", "keyword": ";a"}]"#)
            .unwrap();

        let (out, summary) = merge(&held, arriving, NOW);

        assert_eq!(out[0].keyword, ";a");
        assert_eq!(summary.keywords_taken, 0);
    }

    #[test]
    fn importing_nothing_changes_nothing() {
        let held = vec![snippet("a", "One", ";a", "first")];
        let (out, summary) = merge(&held, Vec::new(), NOW);

        assert_eq!(out, held);
        assert_eq!(summary, Summary::default());
    }

    #[test]
    fn nothing_already_here_is_ever_removed_by_an_import() {
        // The property that matters most. An import is additive: whatever it
        // does with what arrives, everything already held is still there
        // afterwards, under the same id.
        let held = vec![
            snippet("a", "One", ";a", "first"),
            snippet("b", "Two", ";b", "second"),
        ];
        let arriving = parse(
            r#"[
                {"name": "Three", "text": "third", "keyword": ";a"},
                {"id": "b", "name": "Two renamed", "text": "second again"}
            ]"#,
        )
        .unwrap();

        let (out, _) = merge(&held, arriving, NOW);

        for was in &held {
            assert!(
                out.iter().any(|now| now.id == was.id),
                "{} disappeared",
                was.id
            );
        }
    }
}
