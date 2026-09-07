//! Sending a picture somewhere that answers with a link to it.
//!
//! **This is the one thing Sill does that puts a picture of somebody's screen
//! on a machine that is not theirs.** So the shape of the module is set by
//! that rather than by convenience: nothing is configured out of the box, no
//! credential is shipped, private mode refuses it the way it refuses a
//! capture, and the upload happens because a person pressed a button rather
//! than because a picture was taken.
//!
//! ## Why two providers and not ten
//!
//! Imgur, because it is the one people already have, and a custom endpoint,
//! because that is what makes this useful without Sill shipping anybody's
//! credentials or picking a favourite. The custom shape is ShareX's, which is
//! the format the services themselves publish instructions for: a URL, the
//! form field the file goes in, and where the link is in the answer.
//!
//! ## What is here and what is not
//!
//! Everything except the request. Deciding what to send, and reading the link
//! out of what came back, are the parts that can be wrong, so they are pure
//! functions with tests and the network call is the thin part around them.

/// Where a picture is being sent, worked out from the preferences.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// Imgur's anonymous endpoint, with the application id to charge it to.
    Imgur { client_id: String },
    /// Somebody's own endpoint, in ShareX's shape.
    Custom {
        url: String,
        field: String,
        /// A dotted path into the JSON answer, or empty for the whole body.
        json_path: String,
    },
}

/// Why a picture is not going anywhere.
///
/// Named cases rather than one string, because these are the two a person can
/// do something about and the settings row that fixes each one is different.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// No provider has been chosen, which is what off looks like.
    NotSetUp,
    /// A provider is chosen and the field it needs is empty.
    Incomplete,
}

impl Refusal {
    pub fn reason(self) -> &'static str {
        match self {
            Refusal::NotSetUp => {
                "No upload service is set up. Settings, Screenshots, Sharing names where a \
                 picture goes."
            }
            Refusal::Incomplete => {
                "The upload service is missing something it needs. Settings, Screenshots, \
                 Sharing has the field that is empty."
            }
        }
    }
}

/// Where this picture is going, or why it is not going anywhere.
///
/// Trimmed on the way in. A pasted client id or URL routinely carries a
/// trailing newline, and an id with one on it fails at the far end with an
/// authorisation error that names nothing.
pub fn target(upload: &crate::preferences::Upload) -> Result<Target, Refusal> {
    match upload.provider.trim() {
        "imgur" => {
            let client_id = upload.imgur_client_id.trim();
            if client_id.is_empty() {
                return Err(Refusal::Incomplete);
            }

            Ok(Target::Imgur {
                client_id: client_id.to_string(),
            })
        }

        "custom" => {
            let url = upload.custom_url.trim();
            if url.is_empty() {
                return Err(Refusal::Incomplete);
            }

            Ok(Target::Custom {
                url: url.to_string(),
                // The name most services use, so somebody who leaves it blank
                // gets the common case rather than an error.
                field: match upload.custom_field.trim() {
                    "" => "file".to_string(),
                    named => named.to_string(),
                },
                json_path: upload.custom_json_path.trim().to_string(),
            })
        }

        _ => Err(Refusal::NotSetUp),
    }
}

/// The link inside an answer, following a dotted path.
///
/// An empty path means the body is the link, which is what the simplest
/// services answer with. A path that leads nowhere is an error naming the path
/// rather than an empty string, because a blank link on the clipboard is the
/// version of this failure somebody discovers by pasting it into a message.
pub fn link_in(body: &str, path: &str) -> Result<String, String> {
    let path = path.trim();

    if path.is_empty() {
        let found = body.trim();
        return if found.is_empty() {
            Err("the service answered with nothing".to_string())
        } else {
            Ok(found.to_string())
        };
    }

    let value: serde_json::Value = serde_json::from_str(body)
        .map_err(|_| format!("the service did not answer with JSON: {}", cut(body)))?;

    let mut at = &value;
    for step in path.split('.') {
        at = at
            .get(step)
            .ok_or_else(|| format!("there is no {path} in what the service answered"))?;
    }

    at.as_str()
        .map(str::to_string)
        .ok_or_else(|| format!("{path} is not a link in what the service answered"))
}

/// As much of an unexpected answer as belongs in a message.
///
/// A service that is down answers with a whole HTML page, and putting that in
/// a status line makes the window unreadable. Bounded on a character boundary
/// rather than a byte, or a multi-byte character at the cut panics.
fn cut(body: &str) -> String {
    const ENOUGH: usize = 120;

    let trimmed = body.trim();
    match trimmed.char_indices().nth(ENOUGH) {
        Some((at, _)) => format!("{}...", &trimmed[..at]),
        None => trimmed.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::preferences::Upload;

    fn imgur(id: &str) -> Upload {
        Upload {
            provider: "imgur".to_string(),
            imgur_client_id: id.to_string(),
            ..Upload::default()
        }
    }

    /// Nothing configured is off, and off says so in words somebody can act on.
    ///
    /// The default matters more than most: it is what every install has until
    /// somebody deliberately changes it, and the whole argument for shipping
    /// this feature at all is that it does nothing until then.
    #[test]
    fn a_fresh_install_uploads_nowhere() {
        assert_eq!(target(&Upload::default()), Err(Refusal::NotSetUp));
        assert_eq!(
            crate::preferences::Screenshot::default().upload,
            Upload::default(),
            "a screenshot section that has never been touched must not name a service"
        );
    }

    /// A provider chosen but not finished is a different failure from no
    /// provider, because the row that fixes it is a different row.
    #[test]
    fn a_service_missing_what_it_needs_is_told_apart_from_no_service() {
        assert_eq!(target(&imgur("")), Err(Refusal::Incomplete));
        assert_eq!(
            target(&Upload {
                provider: "custom".to_string(),
                ..Upload::default()
            }),
            Err(Refusal::Incomplete)
        );

        assert_ne!(Refusal::Incomplete.reason(), Refusal::NotSetUp.reason());
    }

    /// A pasted credential carries whitespace, and the far end will not say so.
    #[test]
    fn a_pasted_id_with_a_newline_on_it_still_works() {
        assert_eq!(
            target(&imgur("  abc123\n")),
            Ok(Target::Imgur {
                client_id: "abc123".to_string()
            })
        );
    }

    /// The form field falls back to the name nearly every service uses.
    #[test]
    fn a_custom_service_with_no_field_named_sends_the_usual_one() {
        let found = target(&Upload {
            provider: "custom".to_string(),
            custom_url: "https://example.test/up".to_string(),
            ..Upload::default()
        });

        assert_eq!(
            found,
            Ok(Target::Custom {
                url: "https://example.test/up".to_string(),
                field: "file".to_string(),
                json_path: String::new(),
            })
        );
    }

    /// The link is read out of a nested answer, which is Imgur's shape.
    #[test]
    fn a_json_path_reads_the_url_out_of_a_nested_answer() {
        let body = r#"{"status":200,"data":{"link":"https://i.example.test/a.png"}}"#;

        assert_eq!(
            link_in(body, "data.link"),
            Ok("https://i.example.test/a.png".to_string())
        );
    }

    /// An empty path means the body is the link.
    #[test]
    fn no_path_means_the_answer_is_the_link() {
        assert_eq!(
            link_in("  https://example.test/a.png\n", ""),
            Ok("https://example.test/a.png".to_string())
        );
    }

    /// A path that leads nowhere is an error, never an empty link.
    ///
    /// This is the failure that is worst when it is quiet: a blank string on
    /// the clipboard looks like a successful upload until it is pasted into a
    /// message somebody has already sent.
    #[test]
    fn a_path_that_leads_nowhere_is_an_error_rather_than_an_empty_link() {
        let body = r#"{"data":{"url":"https://example.test/a.png"}}"#;

        let why = link_in(body, "data.link").expect_err("a missing path answered with a link");
        assert!(why.contains("data.link"), "{why}");
    }

    /// A service answering with a page rather than JSON says so, briefly.
    #[test]
    fn a_service_that_is_down_does_not_put_its_error_page_in_the_status_line() {
        let page = format!("<html><body>{}</body></html>", "unavailable ".repeat(80));

        let why = link_in(&page, "data.link").expect_err("an HTML page parsed as JSON");
        assert!(why.contains("did not answer with JSON"), "{why}");
        assert!(
            why.chars().count() < 200,
            "a whole error page reached the message: {} characters",
            why.chars().count()
        );
    }

    /// Cutting a long answer never lands inside a character.
    #[test]
    fn a_long_answer_is_cut_on_a_character_and_not_on_a_byte() {
        let wide = "é".repeat(400);

        // Slicing a string by a byte count inside a two-byte character panics,
        // and the answer from a service is not something this controls.
        let cut = cut(&wide);
        assert!(cut.ends_with("..."));
        assert!(cut.chars().count() < wide.chars().count());
    }
}
