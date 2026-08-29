//! Turning text into other text.
//!
//! Pure functions, no application, no clipboard, no window. That is the point:
//! these are the fiddly bits people reach for a website to do, and every one
//! of them is a thing that can be got subtly wrong in a way nobody notices
//! until it matters. Keeping them here means they can be tested exhaustively
//! without a running launcher, which is the only way to have any confidence in
//! a base64 decoder.
//!
//! Each returns `Result` rather than a best effort. A decoder handed something
//! that is not what it expects should say so, not hand back mojibake that
//! looks like it worked.

use base64::Engine;

/// Upper case, by the rules of the text's own script.
///
/// `to_uppercase` rather than `to_ascii_uppercase`: the ASCII version leaves
/// every accented character alone, which turns "café" into "CAFé".
pub fn upper(input: &str) -> String {
    input.to_uppercase()
}

pub fn lower(input: &str) -> String {
    input.to_lowercase()
}

/// Capitalises the first letter of each word and lowercases the rest.
///
/// Word boundaries are whitespace and punctuation, so "o'neill-smith" comes
/// back as "O'Neill-Smith" rather than "O'neill-smith". Deliberately
/// mechanical: this is not a style guide, and second-guessing which small
/// words to leave alone would make the result unpredictable.
pub fn title_case(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut starting = true;

    for character in input.chars() {
        if starting {
            out.extend(character.to_uppercase());
        } else {
            out.extend(character.to_lowercase());
        }
        starting = !character.is_alphanumeric();
    }

    out
}

/// Trims each line and drops the blank ones.
///
/// What text copied out of a document or a terminal usually needs, and doing
/// only the outer trim would leave the ragged indentation that made it worth
/// running in the first place.
pub fn tidy_lines(input: &str) -> String {
    input
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn base64_encode(input: &str) -> String {
    base64::engine::general_purpose::STANDARD.encode(input)
}

/// Decodes base64, accepting the URL-safe alphabet and missing padding.
///
/// Both turn up constantly: a JWT segment is URL-safe and unpadded, and a
/// decoder that refuses either is a decoder nobody can use on the thing they
/// most often have.
pub fn base64_decode(input: &str) -> Result<String, String> {
    let trimmed: String = input.split_whitespace().collect();

    let attempts = [
        base64::engine::general_purpose::STANDARD.decode(&trimmed),
        base64::engine::general_purpose::STANDARD_NO_PAD.decode(&trimmed),
        base64::engine::general_purpose::URL_SAFE.decode(&trimmed),
        base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(&trimmed),
    ];

    let bytes = attempts
        .into_iter()
        .flatten()
        .next()
        .ok_or_else(|| "That is not base64".to_string())?;

    String::from_utf8(bytes).map_err(|_| "That decodes to bytes, not text".to_string())
}

/// Percent-encodes for use in a URL.
///
/// The same encoder quicklinks use, so a value encoded here and a value
/// substituted into a saved link come out identical.
pub fn url_encode(input: &str) -> String {
    crate::quicklinks::resolve::percent_encode(input)
}

pub fn url_decode(input: &str) -> Result<String, String> {
    let bytes = input.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;

    while i < bytes.len() {
        match bytes[i] {
            b'%' => {
                let pair = bytes
                    .get(i + 1..i + 3)
                    .and_then(|hex| std::str::from_utf8(hex).ok())
                    .and_then(|hex| u8::from_str_radix(hex, 16).ok())
                    .ok_or_else(|| "That has a % that is not an escape".to_string())?;
                out.push(pair);
                i += 3;
            }
            // `+` means space only in a form-encoded body, not in a path. The
            // ambiguity is real and unresolvable from the text alone; leaving
            // it be is the answer that never corrupts a filename.
            other => {
                out.push(other);
                i += 1;
            }
        }
    }

    String::from_utf8(out).map_err(|_| "That decodes to bytes, not text".to_string())
}

/// Re-indents JSON, keeping the keys in the order they were written.
///
/// Key order is the whole reason this crate builds `serde_json` with its
/// `preserve_order` feature. Without it a round trip sorts every object
/// alphabetically, so formatting a config file quietly rearranges it, which is
/// a destructive surprise from something that promised only to change the
/// whitespace.
pub fn json_pretty(input: &str) -> Result<String, String> {
    let value: serde_json::Value =
        serde_json::from_str(input).map_err(|err| format!("That is not JSON: {err}"))?;
    serde_json::to_string_pretty(&value).map_err(|err| err.to_string())
}

/// Puts JSON on one line.
pub fn json_compact(input: &str) -> Result<String, String> {
    let value: serde_json::Value =
        serde_json::from_str(input).map_err(|err| format!("That is not JSON: {err}"))?;
    serde_json::to_string(&value).map_err(|err| err.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn case_changes_respect_the_script_rather_than_only_ascii() {
        // `to_ascii_uppercase` would leave the accented letters alone and
        // produce "CAFé NAïVE", which looks like a bug in the launcher.
        assert_eq!(upper("café naïve"), "CAFÉ NAÏVE");
        assert_eq!(lower("CAFÉ NAÏVE"), "café naïve");
    }

    #[test]
    fn title_case_treats_punctuation_as_a_word_boundary() {
        assert_eq!(title_case("o'neill-smith"), "O'Neill-Smith");
        assert_eq!(title_case("the QUICK brown fox"), "The Quick Brown Fox");
    }

    #[test]
    fn tidying_lines_removes_the_indentation_that_made_it_worth_doing() {
        assert_eq!(tidy_lines("  one  \n\n\t two\n   \n three "), "one\ntwo\nthree");
        assert_eq!(tidy_lines(""), "");
    }

    #[test]
    fn base64_survives_a_round_trip_including_characters_beyond_ascii() {
        for original in ["hello", "café ☕", "", "a", "line\nbreak"] {
            let encoded = base64_encode(original);
            assert_eq!(
                base64_decode(&encoded).as_deref(),
                Ok(original),
                "round trip failed for {original:?}"
            );
        }
    }

    #[test]
    fn base64_accepts_the_shapes_people_actually_paste() {
        // A JWT segment: URL-safe alphabet, no padding. A decoder that refuses
        // this is refusing the single most common thing anyone decodes.
        assert_eq!(base64_decode("eyJhIjoxfQ").as_deref(), Ok("{\"a\":1}"));
        // Wrapped at some column by whatever printed it.
        assert_eq!(base64_decode("aGVs\nbG8=").as_deref(), Ok("hello"));
    }

    #[test]
    fn base64_refuses_rather_than_returning_rubbish() {
        assert!(base64_decode("not base64 at all !!!").is_err());
        // Valid base64 whose bytes are not text. Handing back mojibake would
        // look like it worked.
        let binary = base64::engine::general_purpose::STANDARD.encode([0xff, 0xfe, 0x00]);
        assert!(base64_decode(&binary).is_err());
    }

    #[test]
    fn url_encoding_round_trips_and_escapes_what_matters() {
        let original = "a b&c=d/e?f#g";
        let encoded = url_encode(original);

        assert!(!encoded.contains(' '), "a raw space survived: {encoded}");
        assert!(!encoded.contains('&'), "a raw ampersand survived: {encoded}");
        assert_eq!(url_decode(&encoded).as_deref(), Ok(original));
    }

    #[test]
    fn url_decoding_leaves_a_plus_alone() {
        // `+` means space in a form body and means `+` in a path. The text
        // alone cannot say which, and guessing corrupts filenames.
        assert_eq!(url_decode("a+b").as_deref(), Ok("a+b"));
        assert_eq!(url_decode("a%2Bb").as_deref(), Ok("a+b"));
    }

    #[test]
    fn url_decoding_refuses_a_broken_escape() {
        assert!(url_decode("%zz").is_err());
        assert!(url_decode("ends with %").is_err());
    }

    #[test]
    fn json_is_reformatted_both_ways_and_refused_when_it_is_not_json() {
        // Deliberately not alphabetical. A round trip that sorts these has
        // rearranged somebody's file while claiming to have reindented it,
        // and that is what `serde_json`'s default BTreeMap backing does.
        let compact = r#"{"b":1,"a":[2,3]}"#;
        let pretty = json_pretty(compact).expect("valid JSON");

        assert!(pretty.contains('\n'), "pretty output is still on one line");

        let b = pretty.find("\"b\"").expect("b survives");
        let a = pretty.find("\"a\"").expect("a survives");
        assert!(b < a, "the keys were reordered:\n{pretty}");

        assert_eq!(json_compact(&pretty).as_deref(), Ok(compact));

        assert!(json_pretty("{definitely not json").is_err());
    }

    #[test]
    fn every_transform_leaves_empty_text_alone_rather_than_failing() {
        // The clipboard is empty more often than anyone expects, and a row of
        // actions that all error on it is worse than ones that do nothing.
        assert_eq!(upper(""), "");
        assert_eq!(lower(""), "");
        assert_eq!(title_case(""), "");
        assert_eq!(base64_encode(""), "");
        assert_eq!(base64_decode("").as_deref(), Ok(""));
        assert_eq!(url_encode(""), "");
        assert_eq!(url_decode("").as_deref(), Ok(""));
    }
}
