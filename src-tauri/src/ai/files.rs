//! Turning a file on disk into something a model can be handed.
//!
//! Two outcomes and one refusal. A picture becomes a data URI, which is the
//! only way a chat-completions request carries one. A text file becomes its
//! text, folded into the question with its name above it, because no service
//! has a content type for a file. Anything else is refused by name rather than
//! sent as a wall of mojibake that costs money and says nothing.

use base64::Engine;

use super::openai::Attached;

/// The largest picture that is worth sending.
///
/// A data URI is base64, so it arrives about a third larger than the file, and
/// every provider counts it as tokens. Four megabytes of PNG is already an
/// expensive question; past that it is nearly always a screenshot that would
/// say the same thing at half the size.
const MOST_IMAGE: usize = 4 * 1024 * 1024;

/// The most text one file may contribute.
///
/// Pasted into the conversation and paid for on every request after it, so the
/// ceiling is the same one the reading tools use.
const MOST_TEXT: usize = 100_000;

/// The ceilings, for anything that has to know them without reading a file.
///
/// A picture pasted from the clipboard never touches the disk, so it cannot go
/// through `read` and would otherwise need its own copy of these numbers.
/// Asking for them is one definition rather than two that drift.
#[derive(Debug, Clone, Copy, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Limits {
    pub image: usize,
    pub text: usize,
}

pub fn limits() -> Limits {
    Limits {
        image: MOST_IMAGE,
        text: MOST_TEXT,
    }
}

/// What a file becomes, or why it cannot become anything.
pub fn read(path: &str) -> Result<Attached, String> {
    let path = std::path::Path::new(path);

    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string_lossy().to_string());

    let data = std::fs::read(path).map_err(|err| format!("Could not read {name}: {err}"))?;
    let bytes = data.len();

    if let Some(mime) = image_type(&data) {
        if bytes > MOST_IMAGE {
            return Err(format!(
                "{name} is {}, and a picture has to be under {} to send.",
                in_words(bytes),
                in_words(MOST_IMAGE),
            ));
        }

        let encoded = base64::engine::general_purpose::STANDARD.encode(&data);

        return Ok(Attached {
            name,
            kind: "image".to_string(),
            body: format!("data:{mime};base64,{encoded}"),
            bytes,
        });
    }

    let text = String::from_utf8_lossy(&data);

    // The same test the reading tool uses: enough replacement characters means
    // it was never text, and a model handed the bytes of an archive as a
    // string will try to reason about them.
    let noise = text
        .chars()
        .filter(|c| *c == char::REPLACEMENT_CHARACTER)
        .count();

    if noise > text.chars().count() / 20 {
        return Err(format!(
            "{name} is not text and not a picture, so there is nothing to send."
        ));
    }

    let whole: String = text.chars().take(MOST_TEXT).collect();

    Ok(Attached {
        name,
        kind: "text".to_string(),
        body: whole,
        bytes,
    })
}

/// What kind of picture this is, read from the bytes rather than the name.
///
/// A file called `.png` that is really a JPEG is common enough, and a provider
/// handed the wrong type answers with a message about the request rather than
/// about the file. The magic numbers are short and stable.
fn image_type(data: &[u8]) -> Option<&'static str> {
    if data.starts_with(&[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]) {
        return Some("image/png");
    }

    if data.starts_with(&[0xff, 0xd8, 0xff]) {
        return Some("image/jpeg");
    }

    if data.starts_with(b"GIF87a") || data.starts_with(b"GIF89a") {
        return Some("image/gif");
    }

    // RIFF....WEBP
    if data.len() > 12 && data.starts_with(b"RIFF") && &data[8..12] == b"WEBP" {
        return Some("image/webp");
    }

    if data.starts_with(b"BM") {
        return Some("image/bmp");
    }

    None
}

/// A size somebody would say out loud.
fn in_words(bytes: usize) -> String {
    if bytes >= 1024 * 1024 {
        return format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0));
    }

    if bytes >= 1024 {
        return format!("{} KB", bytes / 1024);
    }

    format!("{bytes} bytes")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a_file(name: &str, data: &[u8]) -> String {
        let path = std::env::temp_dir().join(format!("sill-attach-{name}"));
        std::fs::write(&path, data).expect("written");
        path.to_string_lossy().to_string()
    }

    /// The eight bytes every PNG begins with, plus enough to look like a file.
    fn a_png() -> Vec<u8> {
        let mut data = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
        data.extend(std::iter::repeat(0u8).take(64));
        data
    }

    mod what_it_becomes {
        use super::*;

        #[test]
        fn a_picture_becomes_a_data_uri() {
            let attached = read(&a_file("shot.png", &a_png())).expect("a picture");
            assert_eq!(attached.kind, "image");
            assert!(attached.body.starts_with("data:image/png;base64,"));
            assert_eq!(attached.name, "sill-attach-shot.png");
        }

        /// Read from the bytes rather than the name. A file called `.png` that
        /// is really a JPEG is common, and a provider handed the wrong type
        /// answers about the request rather than about the file.
        #[test]
        fn the_kind_comes_from_the_bytes_not_the_name() {
            let jpeg = vec![0xff, 0xd8, 0xff, 0xe0, 0x00, 0x10];
            let attached = read(&a_file("lying.png", &jpeg)).expect("a picture");
            assert!(attached.body.starts_with("data:image/jpeg;base64,"));
        }

        #[test]
        fn a_text_file_becomes_its_text() {
            let attached = read(&a_file("notes.txt", b"hello there")).expect("text");
            assert_eq!(attached.kind, "text");
            assert_eq!(attached.body, "hello there");
        }

        /// A model handed the bytes of an archive as a string will try to
        /// reason about them, and it has no way to tell that it should not.
        #[test]
        fn something_that_is_neither_is_refused_by_name() {
            let rubbish: Vec<u8> = (0..200u16).map(|n| (n % 256) as u8).collect();
            let refused = read(&a_file("archive.zip", &rubbish)).expect_err("refused");
            assert!(refused.contains("sill-attach-archive.zip"), "{refused}");
            assert!(refused.contains("not text and not a picture"), "{refused}");
        }

        #[test]
        fn a_file_that_is_not_there_says_so_by_name() {
            let refused = read("C:/nothing/here/at/all.png").expect_err("refused");
            assert!(refused.contains("all.png"), "{refused}");
        }
    }

    mod what_is_too_big {
        use super::*;

        /// A data URI is base64, so it arrives a third larger than the file
        /// and every provider counts it as tokens.
        #[test]
        fn a_picture_past_the_ceiling_is_refused_in_words_anybody_reads() {
            let mut huge = a_png();
            huge.extend(std::iter::repeat(0u8).take(MOST_IMAGE));

            let refused = read(&a_file("huge.png", &huge)).expect_err("refused");
            assert!(refused.contains("MB"), "no size in it: {refused}");
            assert!(
                !refused.contains("4194304"),
                "it said bytes at a person: {refused}"
            );
        }

        /// Text is cut rather than refused, because the first hundred thousand
        /// characters of a log is usually the answer and the whole thing is a
        /// bill on every request after it.
        #[test]
        fn a_long_text_file_is_cut_rather_than_refused() {
            let long = "a".repeat(MOST_TEXT * 2);
            let attached = read(&a_file("long.txt", long.as_bytes())).expect("text");
            assert_eq!(attached.body.chars().count(), MOST_TEXT);
            assert_eq!(
                attached.bytes,
                MOST_TEXT * 2,
                "but it still says how big it was"
            );
        }
    }

    mod sizes_in_words {
        use super::*;

        #[test]
        fn each_step_reads_the_way_somebody_would_say_it() {
            assert_eq!(in_words(512), "512 bytes");
            assert_eq!(in_words(4096), "4 KB");
            assert_eq!(in_words(4 * 1024 * 1024), "4.0 MB");
        }
    }
}
