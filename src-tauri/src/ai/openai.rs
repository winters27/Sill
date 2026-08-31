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

/// Which models this service has.
///
/// Asked rather than typed. A model id is a string like `gemini-3-flash` or
/// `anthropic/claude-sonnet-5`, and one character wrong is a request that
/// fails with a message about a model nobody meant to ask for. Every service
/// that speaks this shape publishes the list, so the choice can be a list.
///
/// Sorted, because a service returns them in whatever order its database felt
/// like and a picker that reorders itself between openings is one nobody can
/// learn.
pub async fn models(client: &reqwest::Client, provider: &Provider) -> Result<Vec<String>, String> {
    super::provider::check(&provider.base_url).map_err(|why| why.message().to_string())?;

    let base = provider.base_url.trim().trim_end_matches('/');
    let mut request = client.get(format!("{base}/models"));

    for (name, value) in headers(provider) {
        request = request.header(name, value);
    }

    let response = request
        .send()
        .await
        .map_err(|err| format!("could not reach {}: {err}", provider.name))?;

    if !response.status().is_success() {
        let status = response.status().as_u16();
        let said = response.text().await.unwrap_or_default();
        return Err(complaint(status, &said));
    }

    let body: serde_json::Value = response
        .json()
        .await
        .map_err(|err| format!("that list could not be read: {err}"))?;

    Ok(model_ids(&body))
}

/// The ids out of a models response.
///
/// Its own function so the shape can be tested without a service. Both the
/// documented `{"data": [...]}` and the bare array some gateways return, in
/// case the second is what arrives.
pub fn model_ids(body: &serde_json::Value) -> Vec<String> {
    let rows = body
        .get("data")
        .and_then(|d| d.as_array())
        .or_else(|| body.as_array());

    let Some(rows) = rows else {
        return Vec::new();
    };

    let mut ids: Vec<String> = rows
        .iter()
        .filter_map(|row| {
            row.get("id")
                .and_then(|id| id.as_str())
                // A bare list of names, which is what a couple of gateways send.
                .or_else(|| row.as_str())
        })
        .filter(|id| !id.is_empty())
        .map(str::to_string)
        .collect();

    ids.sort();
    ids.dedup();
    ids
}

/// What to say when a provider refuses.
///
/// Its own words where there are any, because "400" tells somebody nothing
/// and "model not found" tells them exactly what to fix. Trimmed, because a
/// gateway will return a page of HTML and a status line is not the place.
fn complaint(status: u16, body: &str) -> String {
    /*
     * The three refusals that mean something specific, said in Sill's words.
     *
     * A provider's own JSON is written for whoever is calling the API, and it
     * is the wrong register for a settings window: somebody who pasted the
     * wrong key needs to be told that, not handed a nested error object to
     * read. Everything else falls through to the body, because a message
     * nobody anticipated is more use verbatim than summarised wrongly.
     */
    /*
     * The two refusals worth saying in Sill's words.
     *
     * Rejected credentials are said here rather than passed through, because
     * some services quote the key back in the message and a settings window is
     * not the place for that. Everything else keeps the provider's own words,
     * including a 404: it looks like a wrong address and often is not, since a
     * model that does not exist is a 404 on several services, and telling
     * somebody to check an address that is correct sends them the wrong way.
     */
    match status {
        401 | 403 => return "That key was not accepted.".to_string(),
        429 => return "That provider is rate limiting the request. Try again shortly.".to_string(),
        _ => {}
    }

    // The provider's own sentence, dug out of the object it arrived in.
    //
    // Every service speaking this shape answers a refusal with an error field,
    // and what is in it is usually the most useful thing available: xAI's says
    // where to get a key, and a wrong model name is quoted back exactly as it
    // was sent. What is not useful is the JSON around it.
    if let Some(said) = said_by(body) {
        // The number as an aside rather than the subject. It says nothing on
        // its own, but it is the searchable half when somebody reports one.
        return format!("{said} ({status})");
    }

    let said = body.trim();

    if said.is_empty() {
        return format!("that provider refused the request ({status})");
    }

    let short: String = said.chars().take(300).collect();
    format!("that provider refused the request ({status}): {short}")
}

/// The sentence a provider put in its error body, if it put one there.
///
/// Two shapes, because the services disagree: `{"error": "..."}` and
/// `{"error": {"message": "..."}}`. Anything else answers nothing and the
/// caller falls back to the body as it arrived, which is the right default for
/// a message nobody anticipated.
fn said_by(body: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(body.trim()).ok()?;

    let said = value
        .pointer("/error/message")
        .or_else(|| value.pointer("/error"))
        .or_else(|| value.pointer("/message"))
        .and_then(|found| found.as_str())?
        .trim();

    if said.is_empty() {
        return None;
    }

    Some(said.chars().take(300).collect())
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

    mod what_a_refusal_says {
        use super::*;

        /// The one that happens most, and the one whose real body is least
        /// use: a nested JSON error object about invalid authentication is
        /// not what somebody who pasted the wrong key needs to read.
        #[test]
        fn a_rejected_key_says_that_and_nothing_else() {
            let said = complaint(401, r#"{"error":{"message":"Incorrect API key provided: sk-abc..."}}"#);
            assert_eq!(said, "That key was not accepted.");
            assert!(!said.contains("sk-abc"), "it repeated the key back");
        }

        #[test]
        fn a_forbidden_request_reads_the_same_way() {
            assert_eq!(complaint(403, "{}"), "That key was not accepted.");
        }

        /// A 404 is not proof the address is wrong. Several services answer
        /// one for a model that does not exist, and sending somebody to check
        /// an address that is correct sends them the wrong way.
        #[test]
        fn a_404_is_not_read_as_a_wrong_address() {
            let said = complaint(404, r#"{"error":{"message":"model \"qwen9\" not found"}}"#);
            assert!(said.contains("qwen9"), "it said {said:?}");
            assert!(!said.to_lowercase().contains("address"), "it guessed: {said:?}");
        }

        #[test]
        fn being_rate_limited_says_to_come_back() {
            assert!(complaint(429, "slow down").contains("rate limiting"));
        }

        /// What xAI actually answers a bad key with, taken from a real
        /// request. The status is 400, not 401, which is why mapping by status
        /// alone was not enough: the services disagree about which number a
        /// rejected key is, and only the body says what happened.
        #[test]
        fn a_provider_that_calls_a_bad_key_a_bad_request_still_reads_plainly() {
            let said = complaint(
                400,
                r#"{"code":"invalid-argument","error":"Incorrect API key provided. You can obtain an API key from https://console.x.ai."}"#,
            );

            assert_eq!(
                said,
                "Incorrect API key provided. You can obtain an API key from \
                 https://console.x.ai. (400)",
            );
        }

        /// The other shape, which is what OpenAI and most of the rest send.
        #[test]
        fn a_nested_message_is_dug_out_of_its_object() {
            let said = complaint(400, r#"{"error":{"message":"model `grok-9` does not exist","type":"invalid_request_error"}}"#);
            assert_eq!(said, "model `grok-9` does not exist (400)");
            assert!(!said.contains("invalid_request_error"), "the object came too");
        }

        /// A body that is not JSON at all is still the most useful thing there
        /// is, so it survives.
        #[test]
        fn something_that_is_not_json_is_passed_through() {
            let said = complaint(502, "upstream connect error");
            assert!(said.contains("upstream connect error"));
        }

        /// An error field that is an empty string is not a message.
        #[test]
        fn an_empty_message_falls_back_rather_than_saying_nothing() {
            let said = complaint(400, r#"{"error":{"message":"  "}}"#);
            assert!(said.contains("400"), "it said {said:?}");
        }

        /// Anything unanticipated is passed through rather than summarised
        /// wrongly. A wrong model name and an unpaid account are both 400,
        /// and only the provider knows which.
        #[test]
        fn anything_else_keeps_the_providers_own_words() {
            let said = complaint(400, r#"{"error":"model `grok-9` does not exist"}"#);
            assert!(said.contains("grok-9"), "the useful half was thrown away");
        }

        #[test]
        fn an_empty_body_still_names_the_status() {
            assert!(complaint(500, "   ").contains("500"));
        }

        /// A provider that answers an error with a page of HTML must not put
        /// a page of HTML in a settings window.
        #[test]
        fn an_enormous_body_is_cut_down() {
            let said = complaint(400, &"x".repeat(5000));
            assert!(said.chars().count() < 400, "it was {} long", said.chars().count());
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

    mod listing_the_models {
        use super::*;

        #[test]
        fn the_documented_shape_reads() {
            let body = serde_json::json!({
                "object": "list",
                "data": [
                    {"id": "gpt-5.2", "object": "model"},
                    {"id": "gpt-5.2-mini", "object": "model"},
                ],
            });

            assert_eq!(model_ids(&body), vec!["gpt-5.2", "gpt-5.2-mini"]);
        }

        /// A picker that reorders itself between openings is one nobody can
        /// learn, and services return these in whatever order they like.
        #[test]
        fn they_come_back_sorted() {
            let body = serde_json::json!({"data": [{"id": "zeta"}, {"id": "alpha"}]});
            assert_eq!(model_ids(&body), vec!["alpha", "zeta"]);
        }

        #[test]
        fn a_bare_array_reads_too() {
            let body = serde_json::json!([{"id": "a"}, "b"]);
            assert_eq!(model_ids(&body), vec!["a", "b"]);
        }

        /// Nothing usable is an empty list, not a failure: the panel then
        /// offers a text field instead of a picker, which still works.
        #[test]
        fn something_unrecognisable_is_no_models_rather_than_an_error() {
            for body in [
                serde_json::json!({}),
                serde_json::json!({"data": "not a list"}),
                serde_json::json!({"data": [{"name": "no id here"}]}),
                serde_json::json!(null),
            ] {
                assert!(model_ids(&body).is_empty(), "{body}");
            }
        }

        #[test]
        fn the_same_model_listed_twice_appears_once() {
            let body = serde_json::json!({"data": [{"id": "a"}, {"id": "a"}]});
            assert_eq!(model_ids(&body), vec!["a"]);
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
