//! Builds the HTTP request that transcribes a clip, per provider.
//!
//! Mirrors `ai::models::provider_model_request`: a pure function from a
//! provider id plus its `ProviderConfig` to a request *description*, with no
//! I/O. `transcriber` executes it. Keeping the two apart is what lets every
//! provider's URL, auth, and form fields be asserted in a unit test rather
//! than discovered against a live endpoint.
//!
//! Every provider here speaks the OpenAI-compatible multipart form, which is
//! what makes one code path serve OpenAI, Groq, OpenRouter, any BYOK gateway,
//! and whisper.cpp's own server. Providers that need a different shape
//! (Google inlines base64 in JSON; Azure builds a deployment URL) are not
//! wired yet and are rejected rather than half-supported.

use crate::dictation::error::DictationError;
use crate::dictation::provider::normalize_openai_base;
use crate::dictation::provider::ProviderConfig;
use std::collections::HashMap;

/// Upload ceiling the OpenAI-compatible transcription endpoints document.
/// Checked before sending so an oversized clip fails instantly instead of
/// after the user waits through the upload.
const CLOUD_UPLOAD_LIMIT: usize = 25 * 1024 * 1024;

/// What the caller wants from this particular transcription, as opposed to
/// the provider's standing configuration.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TranscribeOptions {
    /// Overrides the provider's configured model when set.
    pub model: Option<String>,
    /// ISO language code. `None` lets the model auto-detect.
    pub language: Option<String>,
    /// Biasing prompt, e.g. a vocabulary of names the model keeps missing.
    pub prompt: Option<String>,
}

/// How the audio and its parameters are carried.
///
/// One variant today. It stays an enum because Google's transcription API
/// takes base64 inlined in JSON rather than a multipart file, so the shape
/// genuinely varies once that provider lands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RequestBody {
    Multipart {
        /// Form field the audio itself goes in.
        file_field: String,
        /// Filename to declare for that part. Some servers sniff the
        /// container from the extension rather than the content type.
        filename: String,
        /// Text fields accompanying the audio.
        fields: Vec<(String, String)>,
    },
}

/// A fully-described transcription request, ready to execute.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranscriptionRequest {
    pub url: String,
    pub headers: HashMap<String, String>,
    pub body: RequestBody,
    /// `None` means unlimited, which is only ever true of our own sidecar.
    pub max_upload_bytes: Option<usize>,
}

/// Builds the request that transcribes a clip through `provider_id`.
pub fn transcription_request(
    provider_id: &str,
    config: &ProviderConfig,
    opts: &TranscribeOptions,
) -> Result<TranscriptionRequest, DictationError> {
    match provider_id {
        "openai" => cloud_request(provider_id, config, opts, "https://api.openai.com", &[]),
        "groq" => cloud_request(
            provider_id,
            config,
            opts,
            "https://api.groq.com/openai/v1",
            &[],
        ),
        // The attribution headers match `ai::models::provider_model_request`'s
        // openrouter arm, so transcription shows up under the same app.
        "openrouter" => cloud_request(
            provider_id,
            config,
            opts,
            "https://openrouter.ai/api/v1",
            &[("HTTP-Referer", "https://asyar.app"), ("X-Title", "Asyar")],
        ),
        "custom" => {
            let base = required_base_url(config, "The custom transcription endpoint")?;
            cloud_request(provider_id, config, opts, &base, &[])
        }
        "local" => local_request(config, opts),
        other => Err(DictationError::Validation(format!(
            "Unknown transcription provider '{other}'"
        ))),
    }
}

/// The OpenAI-compatible shape: `POST {base}/audio/transcriptions`, bearer
/// auth, multipart with a `model`.
fn cloud_request(
    provider_id: &str,
    config: &ProviderConfig,
    opts: &TranscribeOptions,
    default_base: &str,
    extra_headers: &[(&str, &str)],
) -> Result<TranscriptionRequest, DictationError> {
    let base = config
        .base_url
        .as_deref()
        .map(str::trim)
        .filter(|base| !base.is_empty())
        .unwrap_or(default_base);

    let model = opts
        .model
        .as_deref()
        .or(config.last_model_id.as_deref())
        .map(str::trim)
        .filter(|model| !model.is_empty())
        .ok_or_else(|| {
            DictationError::Validation(format!(
                "No transcription model is selected for {provider_id}"
            ))
        })?
        .to_string();

    let mut headers = HashMap::new();
    // An empty bearer is worse than none: some gateways answer 401 rather
    // than treating the endpoint as unauthenticated.
    if let Some(key) = config
        .api_key
        .as_deref()
        .map(str::trim)
        .filter(|key| !key.is_empty())
    {
        headers.insert("Authorization".to_string(), format!("Bearer {key}"));
    }
    for (name, value) in extra_headers {
        headers.insert((*name).to_string(), (*value).to_string());
    }

    let mut fields = vec![
        ("model".to_string(), model),
        ("response_format".to_string(), "json".to_string()),
    ];
    push_optional(&mut fields, "language", opts.language.as_deref());
    push_optional(&mut fields, "prompt", opts.prompt.as_deref());

    Ok(TranscriptionRequest {
        url: format!("{}/audio/transcriptions", normalize_openai_base(base)),
        headers,
        body: multipart(fields),
        max_upload_bytes: Some(CLOUD_UPLOAD_LIMIT),
    })
}

/// whisper.cpp's bundled server: `POST /inference`, no auth, and no `model`
/// because it serves whichever model it was started with.
fn local_request(
    config: &ProviderConfig,
    opts: &TranscribeOptions,
) -> Result<TranscriptionRequest, DictationError> {
    let base = required_base_url(config, "The local whisper server")?;

    let mut fields = vec![("response_format".to_string(), "json".to_string())];
    push_optional(&mut fields, "language", opts.language.as_deref());
    push_optional(&mut fields, "prompt", opts.prompt.as_deref());

    Ok(TranscriptionRequest {
        url: format!("{}/inference", base.trim_end_matches('/')),
        headers: HashMap::new(),
        body: multipart(fields),
        max_upload_bytes: None,
    })
}

fn required_base_url(config: &ProviderConfig, label: &str) -> Result<String, DictationError> {
    config
        .base_url
        .as_deref()
        .map(str::trim)
        .filter(|base| !base.is_empty())
        .map(str::to_string)
        .ok_or_else(|| DictationError::Validation(format!("{label} has no address configured")))
}

fn push_optional(fields: &mut Vec<(String, String)>, name: &str, value: Option<&str>) {
    if let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) {
        fields.push((name.to_string(), value.to_string()));
    }
}

fn multipart(fields: Vec<(String, String)>) -> RequestBody {
    RequestBody::Multipart {
        file_field: "file".to_string(),
        filename: "audio.wav".to_string(),
        fields,
    }
}

/// Flattens a transcript to a single line.
///
/// whisper.cpp returns one segment per line, so a two-sentence dictation
/// arrives with a newline in the middle of it. Pasted into a chat box that
/// sends the message early; pasted into a form it breaks the field. Segment
/// boundaries carry no meaning worth preserving here, so every run of
/// whitespace becomes one space.
fn collapse_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Pulls the transcript out of a provider response.
///
/// Every supported provider answers `{"text": ...}`, with OpenRouter adding a
/// `usage` object alongside it. A missing `text` is an error rather than an
/// empty transcript: pasting nothing is indistinguishable from a broken
/// microphone, so the failure has to be loud.
pub fn parse_transcript(body: &str) -> Result<String, DictationError> {
    let value: serde_json::Value = serde_json::from_str(body).map_err(|e| {
        DictationError::Other(format!("Transcription response was not valid JSON: {e}"))
    })?;

    match value.get("text").and_then(serde_json::Value::as_str) {
        Some(text) => Ok(collapse_whitespace(text)),
        None => {
            // Providers report failures in their own envelopes; surfacing the
            // message beats a generic parse error the user cannot act on.
            let reported = value
                .pointer("/error/message")
                .and_then(serde_json::Value::as_str)
                .or_else(|| value.get("error").and_then(serde_json::Value::as_str));
            Err(DictationError::Other(match reported {
                Some(message) => format!("Transcription failed: {message}"),
                None => "Transcription response contained no text".to_string(),
            }))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dictation::provider::ProviderConfig;

    fn config(api_key: &str, base_url: Option<&str>, model: Option<&str>) -> ProviderConfig {
        ProviderConfig {
            enabled: true,
            name: None,
            provider_type: None,
            api_key: Some(api_key.to_string()),
            base_url: base_url.map(str::to_string),
            last_model_id: model.map(str::to_string),
        }
    }

    fn opts() -> TranscribeOptions {
        TranscribeOptions::default()
    }

    fn field<'a>(request: &'a TranscriptionRequest, name: &str) -> Option<&'a str> {
        match &request.body {
            RequestBody::Multipart { fields, .. } => fields
                .iter()
                .find(|(key, _)| key == name)
                .map(|(_, value)| value.as_str()),
        }
    }

    // ── OpenAI ──────────────────────────────────────────────────────────────

    #[test]
    fn openai_posts_to_the_documented_transcriptions_path() {
        let request = transcription_request(
            "openai",
            &config("sk-test", None, Some("whisper-1")),
            &opts(),
        )
        .unwrap();

        assert_eq!(
            request.url,
            "https://api.openai.com/v1/audio/transcriptions"
        );
        assert_eq!(
            request.headers.get("Authorization").map(String::as_str),
            Some("Bearer sk-test")
        );
        assert_eq!(field(&request, "model"), Some("whisper-1"));
    }

    #[test]
    fn openai_honours_a_custom_base_url_without_doubling_the_version_segment() {
        // A user pasting a gateway URL that already ends in /v1 must not get
        // `/v1/v1/audio/transcriptions`.
        let request = transcription_request(
            "openai",
            &config(
                "sk-test",
                Some("https://gateway.internal/v1"),
                Some("whisper-1"),
            ),
            &opts(),
        )
        .unwrap();

        assert_eq!(
            request.url,
            "https://gateway.internal/v1/audio/transcriptions"
        );
    }

    // ── Groq ────────────────────────────────────────────────────────────────

    #[test]
    fn groq_uses_its_openai_compatible_base() {
        let request = transcription_request(
            "groq",
            &config("gsk-test", None, Some("whisper-large-v3-turbo")),
            &opts(),
        )
        .unwrap();

        assert_eq!(
            request.url,
            "https://api.groq.com/openai/v1/audio/transcriptions"
        );
        assert_eq!(field(&request, "model"), Some("whisper-large-v3-turbo"));
    }

    // ── OpenRouter ──────────────────────────────────────────────────────────

    #[test]
    fn openrouter_sends_the_attribution_headers_the_rest_of_the_app_sends() {
        // Matches `ai::models::provider_model_request`'s openrouter arm, which
        // already identifies Asyar to OpenRouter's dashboard.
        let request = transcription_request(
            "openrouter",
            &config("or-test", None, Some("openai/whisper-1")),
            &opts(),
        )
        .unwrap();

        assert_eq!(
            request.url,
            "https://openrouter.ai/api/v1/audio/transcriptions"
        );
        assert_eq!(
            request.headers.get("HTTP-Referer").map(String::as_str),
            Some("https://asyar.app")
        );
        assert_eq!(
            request.headers.get("X-Title").map(String::as_str),
            Some("Asyar")
        );
    }

    // ── Local whisper-server ────────────────────────────────────────────────

    #[test]
    fn local_posts_to_inference_and_sends_no_model_or_auth() {
        // whisper.cpp's server exposes /inference, loads its model from the
        // command line, and has no auth. Sending `model` would be ignored at
        // best and rejected at worst.
        let request = transcription_request(
            "local",
            &config("", Some("http://127.0.0.1:8791"), None),
            &opts(),
        )
        .unwrap();

        assert_eq!(request.url, "http://127.0.0.1:8791/inference");
        assert!(!request.headers.contains_key("Authorization"));
        assert_eq!(field(&request, "model"), None);
    }

    #[test]
    fn local_requires_a_base_url() {
        let err = transcription_request("local", &config("", None, None), &opts());

        assert!(err.is_err(), "the sidecar port is only known at runtime");
    }

    // ── Custom / BYOK ───────────────────────────────────────────────────────

    #[test]
    fn custom_requires_a_base_url() {
        assert!(transcription_request("custom", &config("k", None, Some("m")), &opts()).is_err());
    }

    #[test]
    fn custom_omits_authorization_when_no_key_is_configured() {
        // A self-hosted OpenAI-compatible endpoint on the LAN often has no key.
        // Sending `Bearer ` with an empty value makes some servers 401.
        let mut cfg = config("", Some("http://192.168.50.21:9000/v1"), Some("whisper-1"));
        cfg.api_key = None;

        let request = transcription_request("custom", &cfg, &opts()).unwrap();

        assert!(!request.headers.contains_key("Authorization"));
    }

    // ── Shared behaviour ────────────────────────────────────────────────────

    #[test]
    fn every_provider_asks_for_json_so_one_parser_serves_them_all() {
        for provider in ["openai", "groq", "openrouter", "local"] {
            let request = transcription_request(
                provider,
                &config("k", Some("http://127.0.0.1:1/v1"), Some("m")),
                &opts(),
            )
            .unwrap();

            assert_eq!(
                field(&request, "response_format"),
                Some("json"),
                "{provider} must request json"
            );
        }
    }

    #[test]
    fn language_is_sent_only_when_the_user_pinned_one() {
        let auto = transcription_request("groq", &config("k", None, Some("m")), &opts()).unwrap();
        assert_eq!(field(&auto, "language"), None, "absent means auto-detect");

        let pinned = transcription_request(
            "groq",
            &config("k", None, Some("m")),
            &TranscribeOptions {
                language: Some("en".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(field(&pinned, "language"), Some("en"));
    }

    #[test]
    fn the_options_model_overrides_the_configured_one() {
        let request = transcription_request(
            "groq",
            &config("k", None, Some("whisper-large-v3")),
            &TranscribeOptions {
                model: Some("whisper-large-v3-turbo".into()),
                ..Default::default()
            },
        )
        .unwrap();

        assert_eq!(field(&request, "model"), Some("whisper-large-v3-turbo"));
    }

    #[test]
    fn a_cloud_provider_without_a_model_is_rejected_before_the_upload() {
        // Failing here costs nothing; failing after uploading a 3 MB clip
        // costs the user the whole dictation.
        assert!(transcription_request("groq", &config("k", None, None), &opts()).is_err());
    }

    #[test]
    fn unknown_providers_are_rejected() {
        assert!(transcription_request("hal9000", &config("k", None, Some("m")), &opts()).is_err());
    }

    #[test]
    fn cloud_providers_cap_uploads_and_local_does_not() {
        let cloud = transcription_request("groq", &config("k", None, Some("m")), &opts()).unwrap();
        let local =
            transcription_request("local", &config("", Some("http://x:1"), None), &opts()).unwrap();

        assert_eq!(cloud.max_upload_bytes, Some(25 * 1024 * 1024));
        assert_eq!(local.max_upload_bytes, None, "no cap on our own sidecar");
    }

    // ── Response parsing ────────────────────────────────────────────────────

    #[test]
    fn parses_the_text_field_every_provider_returns() {
        assert_eq!(
            parse_transcript(r#"{"text":"hello world"}"#).unwrap(),
            "hello world"
        );
    }

    #[test]
    fn parses_openrouter_responses_that_carry_a_usage_object() {
        let body = r#"{"text":"hi","usage":{"seconds":9.2,"cost":0.000508}}"#;

        assert_eq!(parse_transcript(body).unwrap(), "hi");
    }

    #[test]
    fn collapses_the_segment_newlines_whisper_returns() {
        // whisper-server joins its segments with newlines. Pasting one
        // straight into a chat box sends the message mid-sentence, so
        // internal runs of whitespace collapse to a single space.
        // Observed live: "...see how it\n works."
        let body = r#"{"text":"See how it\n works."}"#;

        assert_eq!(parse_transcript(body).unwrap(), "See how it works.");
    }

    #[test]
    fn collapses_runs_of_spaces_and_tabs_too() {
        let body = "{\"text\":\"one  two\\t\\tthree\\n\\nfour\"}";

        assert_eq!(parse_transcript(body).unwrap(), "one two three four");
    }

    #[test]
    fn trims_surrounding_whitespace_from_the_transcript() {
        // whisper.cpp habitually returns a leading space before the first word.
        assert_eq!(
            parse_transcript(r#"{"text":"  spoken words \n"}"#).unwrap(),
            "spoken words"
        );
    }

    #[test]
    fn a_response_without_a_text_field_is_an_error_not_an_empty_transcript() {
        // Silently pasting nothing would look like the microphone failed.
        assert!(parse_transcript(r#"{"error":{"message":"bad key"}}"#).is_err());
    }

    #[test]
    fn malformed_json_is_an_error() {
        assert!(parse_transcript("not json at all").is_err());
    }
}
