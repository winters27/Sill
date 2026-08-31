//! The shape nearly everything speaks.
//!
//! OpenAI's chat completions, which xAI, Ollama, OpenRouter, LM Studio and
//! Google's compatibility route all accept. One adapter, six services and
//! anything else somebody points it at.
//!
//! The two halves that can go wrong quietly are both pure functions here: what
//! is sent, and how a line of the response is read. The part that cannot be a
//! pure function, the request itself, does as little as possible.

use serde::{Deserialize, Serialize};

use super::provider::Provider;

/// Who said what.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Message {
    /// `system`, `user` or `assistant`.
    pub role: String,
    pub content: String,
}

impl Message {
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: "user".into(),
            content: content.into(),
        }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: "assistant".into(),
            content: content.into(),
        }
    }

    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: "system".into(),
            content: content.into(),
        }
    }
}

/// What one line of the response said.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// More of the answer.
    Text(String),
    /// The answer is finished.
    Done,
    /// Nothing this needs to act on.
    Ignored,
}

/// Where to send it.
///
/// A base URL is given as the service's own documentation gives it, which for
/// some ends in `/v1` and for others does not. Joining without checking
/// produces `/v1/v1/chat/completions`, which answers 404 with nothing in the
/// message explaining why. The dictation side learned this already; this is
/// the same trap in the same shape.
pub fn endpoint(base_url: &str) -> String {
    let base = base_url.trim().trim_end_matches('/');
    format!("{base}/chat/completions")
}

/// What gets posted.
pub fn body(provider: &Provider, messages: &[Message]) -> serde_json::Value {
    serde_json::json!({
        "model": provider.model,
        "messages": messages,
        // Tokens as they are produced. A launcher that shows nothing for four
        // seconds and then a paragraph feels broken even when it is not.
        "stream": true,
    })
}

/// Reads one line of the event stream.
///
/// Server-sent events: lines beginning `data: `, one JSON object each, and a
/// literal `[DONE]` at the end. Comments, blank lines and any field other than
/// `data` are ignored, which is what the format says to do and also what keeps
/// a future field from breaking the answer arriving.
pub fn parse_line(line: &str) -> Event {
    let line = line.trim();

    let Some(payload) = line.strip_prefix("data:") else {
        return Event::Ignored;
    };

    let payload = payload.trim();

    if payload == "[DONE]" {
        return Event::Done;
    }

    let Ok(value) = serde_json::from_str::<serde_json::Value>(payload) else {
        return Event::Ignored;
    };

    // The delta of the first choice, which is the only one asked for.
    match value.pointer("/choices/0/delta/content").and_then(|c| c.as_str()) {
        Some(text) if !text.is_empty() => Event::Text(text.to_string()),
        _ => Event::Ignored,
    }
}

/// The headers a request needs, beyond the content type.
///
/// A service that needs no key is sent no header rather than an empty one: a
/// local model does not want an `Authorization: Bearer ` with nothing after
/// it, and some gateways reject that rather than ignoring it.
pub fn headers(provider: &Provider) -> Vec<(String, String)> {
    let key = provider.api_key.trim();

    if key.is_empty() {
        return Vec::new();
    }

    vec![("Authorization".to_string(), format!("Bearer {key}"))]
}

/// Asks, and hands each piece of the answer to `on_text` as it arrives.
///
/// Streaming rather than collecting, because the point of a launcher is that
/// it answers while you are still reading the first line. The caller decides
/// what to do with each piece; this knows nothing about windows or events.
///
/// The body is read as bytes and split on newlines here rather than with a
/// line-oriented reader, because a chunk from the network does not arrive on
/// a line boundary and the tail of one chunk is the head of the next.
pub async fn ask(
    client: &reqwest::Client,
    provider: &Provider,
    messages: &[Message],
    mut on_text: impl FnMut(String),
) -> Result<(), String> {
    use futures_util::StreamExt;

    super::provider::check(&provider.base_url).map_err(|why| why.message().to_string())?;

    let mut request = client
        .post(endpoint(&provider.base_url))
        .json(&body(provider, messages));

    for (name, value) in headers(provider) {
        request = request.header(name, value);
    }

    let response = request
        .send()
        .await
        .map_err(|err| format!("could not reach {}: {err}", provider.name))?;

    if !response.status().is_success() {
        let status = response.status();
        // The body, because a provider's own message says far more than the
        // number does: a wrong model name and an unpaid account are both 400.
        let said = response.text().await.unwrap_or_default();
        return Err(complaint(status.as_u16(), &said));
    }

    let mut stream = response.bytes_stream();
    let mut pending = String::new();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|err| format!("the answer stopped part way: {err}"))?;
        pending.push_str(&String::from_utf8_lossy(&chunk));

        // Everything up to the last newline is whole lines; what follows it is
        // the start of the next one.
        while let Some(at) = pending.find('\n') {
            let line: String = pending.drain(..=at).collect();

            match parse_line(&line) {
                Event::Text(text) => on_text(text),
                Event::Done => return Ok(()),
                Event::Ignored => {}
            }
        }
    }

    // The stream ended without a `[DONE]`, which several services do. The
    // answer still arrived.
    Ok(())
}

/// What to say when a provider refuses.
///
/// Its own words where there are any, because "400" tells somebody nothing
/// and "model not found" tells them exactly what to fix. Trimmed, because a
/// gateway will return a page of HTML and a status line is not the place.
fn complaint(status: u16, body: &str) -> String {
    let said = body.trim();

    if said.is_empty() {
        return format!("that provider refused the request ({status})");
    }

    let short: String = said.chars().take(300).collect();
    format!("that provider refused the request ({status}): {short}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn provider(base: &str, key: &str, model: &str) -> Provider {
        Provider {
            base_url: base.into(),
            api_key: key.into(),
            model: model.into(),
            ..Provider::default()
        }
    }

    mod where_it_goes {
        use super::*;

        #[test]
        fn a_base_url_gets_the_path_added() {
            assert_eq!(
                endpoint("https://api.openai.com/v1"),
                "https://api.openai.com/v1/chat/completions",
            );
        }

        /// People paste the URL as their provider's documentation gives it,
        /// and some of them end in a slash.
        #[test]
        fn a_trailing_slash_does_not_double_up() {
            assert_eq!(
                endpoint("http://localhost:11434/v1/"),
                "http://localhost:11434/v1/chat/completions",
            );
            assert_eq!(
                endpoint("  https://api.x.ai/v1  "),
                "https://api.x.ai/v1/chat/completions",
            );
        }
    }

    mod what_is_sent {
        use super::*;

        #[test]
        fn the_model_and_the_conversation_go_in_it() {
            let sent = body(
                &provider("http://x/v1", "", "qwen3:1.7b"),
                &[Message::system("be brief"), Message::user("hello")],
            );

            assert_eq!(sent["model"], "qwen3:1.7b");
            assert_eq!(sent["messages"][0]["role"], "system");
            assert_eq!(sent["messages"][1]["content"], "hello");
        }

        /// A launcher that shows nothing for four seconds and then a paragraph
        /// feels broken even when it is not.
        #[test]
        fn it_always_asks_for_a_stream() {
            let sent = body(&provider("http://x/v1", "", "m"), &[]);
            assert_eq!(sent["stream"], true);
        }

        /// A local model does not want an empty bearer token, and some
        /// gateways reject one rather than ignoring it.
        #[test]
        fn nothing_that_needs_no_key_is_sent_an_empty_one() {
            assert!(headers(&provider("http://localhost:11434/v1", "", "m")).is_empty());
            assert!(headers(&provider("http://x/v1", "   ", "m")).is_empty());
        }

        #[test]
        fn a_key_is_sent_as_a_bearer_token() {
            let sent = headers(&provider("https://x/v1", "sk-abc", "m"));
            assert_eq!(sent, vec![("Authorization".into(), "Bearer sk-abc".into())]);
        }
    }

    mod when_it_refuses {
        use super::*;

        /// "400" tells somebody nothing. "model not found" tells them what to
        /// fix, and a provider's own words are the only place that comes from.
        #[test]
        fn the_providers_own_words_are_kept() {
            let said = complaint(404, r#"{"error":{"message":"model \"qwen9\" not found"}}"#);
            assert!(said.contains("qwen9"), "{said}");
            assert!(said.contains("404"), "{said}");
        }

        #[test]
        fn a_silent_refusal_still_says_something() {
            assert_eq!(
                complaint(500, "   "),
                "that provider refused the request (500)",
            );
        }

        /// A gateway will answer with a page of HTML, and a status line is not
        /// the place for it.
        #[test]
        fn a_wall_of_text_is_cut_rather_than_shown_whole() {
            let said = complaint(502, &"x".repeat(5000));
            assert!(said.len() < 400, "{} characters", said.len());
        }
    }

    mod reading_the_stream {
        use super::*;

        #[test]
        fn a_chunk_of_the_answer_comes_out_as_text() {
            let line = r#"data: {"choices":[{"delta":{"content":"Hel"}}]}"#;
            assert_eq!(parse_line(line), Event::Text("Hel".into()));
        }

        #[test]
        fn the_end_is_marked_plainly() {
            assert_eq!(parse_line("data: [DONE]"), Event::Done);
            assert_eq!(parse_line("data:[DONE]"), Event::Done);
        }

        /// The first chunk carries the role and no content, and the last
        /// carries a finish reason and no content. Neither is text.
        #[test]
        fn a_chunk_with_no_content_is_not_text() {
            for line in [
                r#"data: {"choices":[{"delta":{"role":"assistant"}}]}"#,
                r#"data: {"choices":[{"delta":{},"finish_reason":"stop"}]}"#,
                r#"data: {"choices":[{"delta":{"content":""}}]}"#,
            ] {
                assert_eq!(parse_line(line), Event::Ignored, "{line}");
            }
        }

        /// Comments, blank lines and other fields are part of the format.
        #[test]
        fn everything_that_is_not_data_is_ignored() {
            for line in [
                "",
                "   ",
                ": a comment keeping the connection open",
                "event: message",
                "id: 42",
                "data: not json",
                "{\"choices\":[]}",
            ] {
                assert_eq!(parse_line(line), Event::Ignored, "{line}");
            }
        }

        /// Whitespace around the payload varies between services.
        #[test]
        fn the_space_after_data_is_optional() {
            let with = r#"data: {"choices":[{"delta":{"content":"x"}}]}"#;
            let without = r#"data:{"choices":[{"delta":{"content":"x"}}]}"#;

            assert_eq!(parse_line(with), Event::Text("x".into()));
            assert_eq!(parse_line(without), Event::Text("x".into()));
        }
    }
}
