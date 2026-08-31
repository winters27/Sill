//! Getting quicklinks in and out of a file.
//!
//! The same reasoning as snippets, and the same shape deliberately. Somebody
//! arriving with thirty saved searches is not going to retype them, somebody
//! leaving should not feel trapped, and somebody with two machines should not
//! have to keep them in step by hand.
//!
//! Reading is forgiving and merging is not. A file can be written by anything
//! and should be read as generously as it can be understood; what happens to
//! the links already here afterwards should be exactly one predictable thing.

use serde::Deserialize;

use super::store::Quicklink;

/// One link as a file describes it, before anything is decided about it.
///
/// Every field optional, because this is what a file said rather than what
/// Sill holds. A row with no address is the only one that cannot be used.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Incoming {
    id: Option<String>,
    name: Option<String>,
    /// What another tool might call the same thing.
    title: Option<String>,
    /// What Sill calls the address.
    link: Option<String>,
    /// What other tools call it. All three spellings are common and none is
    /// ambiguous, so all three are read.
    url: Option<String>,
    target: Option<String>,
    keyword: Option<String>,
    /// Another spelling of the same idea.
    shortcut: Option<String>,
    open_with: Option<String>,
}

impl Incoming {
    /// The address, whichever way the file spelled it.
    fn address(&self) -> Option<&str> {
        self.link
            .as_deref()
            .or(self.url.as_deref())
            .or(self.target.as_deref())
            .map(str::trim)
            .filter(|link| !link.is_empty())
    }

    /// What to call it.
    ///
    /// The address itself when the file gave no name, because a row with no
    /// label at all cannot be picked out of a list, and the address is at
    /// least true.
    fn label(&self) -> String {
        let named = self
            .name
            .as_deref()
            .or(self.title.as_deref())
            .map(str::trim)
            .filter(|name| !name.is_empty());

        match named {
            Some(name) => name.to_string(),
            None => self.address().unwrap_or_default().to_string(),
        }
    }

    fn trigger(&self) -> String {
        self.keyword
            .as_deref()
            .or(self.shortcut.as_deref())
            .map(str::trim)
            .unwrap_or_default()
            .to_string()
    }
}

/// What an import changed, counted rather than summarised.
#[derive(Debug, Clone, Copy, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub added: usize,
    /// Already here under the same id, and replaced.
    pub updated: usize,
    /// Already here word for word, and left alone.
    pub skipped: usize,
    /// Arrived with a keyword another link already answers to.
    ///
    /// Imported without it rather than dropped or allowed to collide: the
    /// address is the valuable part, and two links answering one keyword is a
    /// silent coin toss every time it is typed.
    pub keywords_taken: usize,
}

/// Reads a file of quicklinks, however it was written.
///
/// Accepts a bare array or an object with the array under `quicklinks`, and
/// reads any of the three common spellings of an address. Anything with no
/// address is skipped, since there is nowhere for it to go.
pub fn parse(text: &str) -> Result<Vec<Quicklink>, String> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum File {
        Bare(Vec<Incoming>),
        Wrapped { quicklinks: Vec<Incoming> },
    }

    let parsed: File = serde_json::from_str(text)
        .map_err(|err| format!("that file is not quicklinks Sill can read: {err}"))?;

    let rows = match parsed {
        File::Bare(rows) => rows,
        File::Wrapped { quicklinks } => quicklinks,
    };

    Ok(rows
        .into_iter()
        .filter_map(|row| {
            let link = row.address()?.to_string();

            Some(Quicklink {
                id: row.id.clone().unwrap_or_default(),
                name: row.label(),
                link,
                keyword: row.trigger(),
                open_with: row.open_with.clone().unwrap_or_default(),
                uses: 0,
                created: 0,
            })
        })
        .collect())
}

/// Folds imported links into the ones already here.
///
/// Pure, and the whole of the policy. Importing is the one operation that can
/// quietly ruin a set somebody has built up, so what it does is decided in one
/// place with nothing else going on.
///
/// **Nothing is ever removed.** An import adds and replaces; a link that is
/// here and not in the file stays. Somebody importing a colleague's file
/// should not lose their own.
pub fn merge(existing: &[Quicklink], incoming: Vec<Quicklink>, now: i64) -> (Vec<Quicklink>, Summary) {
    let mut out = existing.to_vec();
    let mut summary = Summary::default();

    for mut arriving in incoming {
        // Same id means the same link, wherever it came from.
        let already = out
            .iter()
            .position(|held| !arriving.id.is_empty() && held.id == arriving.id);

        // No id, or an id nothing here uses: is it here word for word anyway?
        let identical = already.is_none()
            && out.iter().any(|held| {
                held.link == arriving.link && held.name == arriving.name
            });

        if identical {
            summary.skipped += 1;
            continue;
        }

        let keyword_taken = !arriving.keyword.is_empty()
            && out.iter().any(|held| {
                held.id != arriving.id && held.keyword.eq_ignore_ascii_case(&arriving.keyword)
            });

        if keyword_taken {
            arriving.keyword = String::new();
            summary.keywords_taken += 1;
        }

        match already {
            Some(at) => {
                // How often it has been used is this machine's history, not
                // the file's. Replacing it with a zero from somebody else's
                // export would throw away the ranking.
                arriving.uses = out[at].uses;
                arriving.created = out[at].created;

                // A link replacing itself keeps its own keyword when the file
                // did not bring one.
                if arriving.keyword.is_empty() {
                    arriving.keyword = out[at].keyword.clone();
                }

                out[at] = arriving;
                summary.updated += 1;
            }
            None => {
                if arriving.id.is_empty() {
                    arriving.id = format!("{now}-{}", out.len());
                }

                if arriving.created == 0 {
                    arriving.created = now;
                }

                out.push(arriving);
                summary.added += 1;
            }
        }
    }

    (out, summary)
}

/// Writes quicklinks as a file another tool has a chance of reading.
///
/// The shape Sill holds, plus `url` beside `link` saying the same thing.
/// Costing a few bytes to be readable by anything that expects the commoner
/// spelling is a better trade than being a format only one program knows.
pub fn to_json(links: &[Quicklink]) -> String {
    let rows: Vec<serde_json::Value> = links
        .iter()
        .map(|link| {
            let mut row = serde_json::json!({
                "id": link.id,
                "name": link.name,
                "link": link.link,
                "url": link.link,
                "keyword": link.keyword,
            });

            // Only when there is one. A link that opens in whatever the system
            // uses should not carry an empty field naming nothing.
            if !link.open_with.is_empty() {
                if let Some(fields) = row.as_object_mut() {
                    fields.insert("openWith".into(), link.open_with.clone().into());
                }
            }

            row
        })
        .collect();

    serde_json::to_string_pretty(&rows).unwrap_or_else(|_| "[]".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_700_000_000;

    fn link(id: &str, name: &str, keyword: &str, address: &str) -> Quicklink {
        Quicklink {
            id: id.into(),
            name: name.into(),
            link: address.into(),
            keyword: keyword.into(),
            open_with: String::new(),
            uses: 0,
            created: NOW,
        }
    }

    // ------------------------------------------------------------- reading

    #[test]
    fn a_file_from_another_tool_reads() {
        let read = parse(
            r#"[{"title": "Search the docs", "url": "https://example.com/?q={query}"}]"#,
        )
        .expect("reads");

        assert_eq!(read.len(), 1);
        assert_eq!(read[0].name, "Search the docs");
        assert_eq!(read[0].link, "https://example.com/?q={query}");
    }

    /// Three spellings of an address are all common and none is ambiguous.
    #[test]
    fn any_spelling_of_the_address_is_understood() {
        for field in ["link", "url", "target"] {
            let text = format!(r#"[{{"name": "One", "{field}": "https://example.com"}}]"#);
            let read = parse(&text).expect("reads");

            assert_eq!(read.len(), 1, "{field} was not read");
            assert_eq!(read[0].link, "https://example.com");
        }
    }

    #[test]
    fn an_array_wrapped_in_an_object_reads_too() {
        let read = parse(r#"{"quicklinks": [{"name": "One", "link": "https://example.com"}]}"#)
            .expect("reads");

        assert_eq!(read.len(), 1);
    }

    /// There is nowhere for it to go, so there is nothing to import.
    #[test]
    fn a_row_with_no_address_is_skipped_rather_than_imported_broken() {
        let read = parse(r#"[{"name": "Nowhere"}, {"name": "One", "link": "https://a.example"}]"#)
            .expect("reads");

        assert_eq!(read.len(), 1);
        assert_eq!(read[0].name, "One");
    }

    /// A row with no label cannot be picked out of a list, and the address is
    /// at least true.
    #[test]
    fn a_link_with_no_name_is_named_after_where_it_goes() {
        let read = parse(r#"[{"link": "https://example.com/search"}]"#).expect("reads");
        assert_eq!(read[0].name, "https://example.com/search");
    }

    #[test]
    fn a_file_sill_wrote_reads_back() {
        let saved = vec![Quicklink {
            open_with: r"C:\Program Files\Browser\browser.exe".into(),
            ..link("one", "Docs", "d", "https://example.com/?q={query}")
        }];

        let read = parse(&to_json(&saved)).expect("reads what it wrote");

        assert_eq!(read.len(), 1);
        assert_eq!(read[0].name, "Docs");
        assert_eq!(read[0].keyword, "d");
        assert_eq!(read[0].link, "https://example.com/?q={query}");
        assert_eq!(read[0].open_with, r"C:\Program Files\Browser\browser.exe");
    }

    /// A link that opens in whatever the system uses should not carry an empty
    /// field naming nothing.
    #[test]
    fn nothing_is_written_for_a_link_that_opens_anywhere() {
        let written = to_json(&[link("one", "Docs", "d", "https://example.com")]);
        assert!(!written.contains("openWith"), "{written}");
    }

    // ------------------------------------------------------------- merging

    #[test]
    fn importing_nothing_changes_nothing() {
        let here = vec![link("one", "Docs", "d", "https://a.example")];
        let (after, summary) = merge(&here, Vec::new(), NOW);

        assert_eq!(after, here);
        assert_eq!(summary.added, 0);
    }

    /// Importing the same file twice must not leave two of everything.
    #[test]
    fn importing_the_same_file_twice_leaves_one_of_each() {
        let incoming = vec![link("one", "Docs", "d", "https://a.example")];

        let (once, first) = merge(&[], incoming.clone(), NOW);
        let (twice, second) = merge(&once, incoming, NOW);

        assert_eq!(first.added, 1);
        assert_eq!(twice.len(), 1);
        assert_eq!(second.added, 0);
    }

    /// Two links answering one keyword is a silent coin toss every time it is
    /// typed, so the arriving one comes in without it.
    #[test]
    fn a_keyword_already_taken_is_dropped_rather_than_allowed_to_collide() {
        let here = vec![link("one", "Docs", "d", "https://a.example")];
        let arriving = vec![link("two", "Other", "d", "https://b.example")];

        let (after, summary) = merge(&here, arriving, NOW);

        assert_eq!(summary.keywords_taken, 1);
        assert_eq!(after.len(), 2);
        assert_eq!(after[0].keyword, "d", "the one already here lost its keyword");
        assert_eq!(after[1].keyword, "");
    }

    /// How often something has been used is this machine's history, not the
    /// file's, and a zero from somebody else's export would throw it away.
    #[test]
    fn a_link_replacing_itself_keeps_its_own_history() {
        let mut here = link("one", "Docs", "d", "https://a.example");
        here.uses = 42;

        let arriving = vec![link("one", "Docs, renamed", "", "https://a.example/new")];
        let (after, summary) = merge(&[here], arriving, NOW);

        assert_eq!(summary.updated, 1);
        assert_eq!(after[0].uses, 42);
        assert_eq!(after[0].name, "Docs, renamed");
        // The file brought no keyword, so it keeps the one it had.
        assert_eq!(after[0].keyword, "d");
    }

    /// Somebody importing a colleague's file should not lose their own.
    #[test]
    fn nothing_already_here_is_ever_removed() {
        let here = vec![
            link("one", "Mine", "m", "https://mine.example"),
            link("two", "Also mine", "a", "https://also.example"),
        ];

        let (after, _) = merge(&here, vec![link("three", "Theirs", "t", "https://x.example")], NOW);

        assert_eq!(after.len(), 3);
        assert!(after.iter().any(|held| held.id == "one"));
        assert!(after.iter().any(|held| held.id == "two"));
    }

    /// A file with no ids still has to be importable twice without doubling.
    #[test]
    fn a_link_that_is_already_here_word_for_word_is_left_alone() {
        let here = vec![link("one", "Docs", "d", "https://a.example")];
        let arriving = vec![link("", "Docs", "d", "https://a.example")];

        let (after, summary) = merge(&here, arriving, NOW);

        assert_eq!(after.len(), 1);
        assert_eq!(summary.skipped, 1);
    }
}
