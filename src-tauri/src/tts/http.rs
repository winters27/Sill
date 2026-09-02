//! Any voice that speaks OpenAI's `/v1/audio/speech`.
//!
//! One request shape and a great many services behind it. OpenAI's own, and
//! every local server that copied the shape rather than inventing one:
//! Kokoro-FastAPI, openedai-speech, LocalAI. So the choice between a paid
//! cloud voice and a neural model running on this machine is a base URL,
//! not a second implementation.
//!
//! The same reasoning the chat window already followed, and the same
//! `ProviderConfig` carrying it, so a key here is sealed by the code that
//! seals that one.

use crate::tts::TtsSettings;

/// What a provider is asked for when nothing else is said.
///
/// OpenAI's cheapest voice model, and the name the compatible servers accept
/// and ignore: Kokoro serves whatever it loaded whatever it is asked for, so a
/// value that is wrong there is harmless and a value that is missing is a 400.
const DEFAULT_MODEL: &str = "tts-1";

/// Turns text into a WAV clip.
pub async fn speak(settings: &TtsSettings, text: &str) -> Result<Vec<u8>, String> {
    let base = settings
        .provider
        .base_url
        .as_deref()
        .map(str::trim)
        .filter(|url| !url.is_empty())
        .ok_or("No address is set for the voice. Put one in Settings.")?;

    // The same rule the chat window follows: plain http to this machine or
    // this network is fine, because a local model has no certificate and
    // never will. Plain http anywhere else carries the key in the clear.
    crate::ai::provider::check(base).map_err(|refused| refused.message().to_string())?;

    let url = format!(
        "{}/audio/speech",
        crate::dictation::provider::normalize_openai_base(base)
    );

    let model = settings
        .provider
        .last_model_id
        .as_deref()
        .map(str::trim)
        .filter(|model| !model.is_empty())
        .unwrap_or(DEFAULT_MODEL);

    let body = serde_json::json!({
        "model": model,
        "input": text,
        "voice": settings.voice,
        // WAV because `PlaySoundW` reads nothing else, and because asking for
        // it is one field against an audio decoding stack.
        "response_format": "wav",
    });

    let mut request = reqwest::Client::new().post(&url).json(&body);

    // A local server usually wants no key at all, so an empty one is left off
    // rather than sent as an empty bearer token, which some servers reject.
    if let Some(key) = settings
        .provider
        .api_key
        .as_deref()
        .map(str::trim)
        .filter(|key| !key.is_empty())
    {
        request = request.bearer_auth(key);
    }

    let response = request
        .send()
        .await
        .map_err(|err| format!("could not reach the voice at {url}: {err}"))?;

    let status = response.status();
    let bytes = response
        .bytes()
        .await
        .map_err(|err| format!("the voice answered but the clip did not arrive: {err}"))?;

    if !status.is_success() {
        return Err(said_why(status, &bytes));
    }

    Ok(bytes.to_vec())
}

/// What went wrong, in the provider's own words where it gave any.
///
/// The body is where the useful half is: "Incorrect API key provided" names
/// the fix and "401" does not. Truncated, because an HTML error page is a
/// kilobyte of markup and the first line of it is enough to recognise.
fn said_why(status: reqwest::StatusCode, body: &[u8]) -> String {
    let text = String::from_utf8_lossy(body);
    let text = text.trim();

    // The shape every OpenAI-compatible server uses for a refusal.
    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(text) {
        if let Some(message) = parsed["error"]["message"].as_str() {
            return format!("the voice refused: {message}");
        }
    }

    if text.is_empty() {
        return format!("the voice refused with {status} and said nothing");
    }

    let mut brief: String = text.chars().take(200).collect();
    if text.chars().count() > 200 {
        brief.push('…');
    }

    format!("the voice refused with {status}: {brief}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_refusal_is_reported_in_the_providers_own_words() {
        let body = br#"{"error":{"message":"Incorrect API key provided: sk-xxx"}}"#;
        let said = said_why(reqwest::StatusCode::UNAUTHORIZED, body);

        assert!(said.contains("Incorrect API key provided"), "{said}");
    }

    /// An HTML error page is what a misconfigured local server answers with.
    #[test]
    fn a_wall_of_html_is_cut_down_rather_than_repeated_whole() {
        let body = "<!DOCTYPE html>".to_string() + &"x".repeat(5_000);
        let said = said_why(reqwest::StatusCode::BAD_GATEWAY, body.as_bytes());

        assert!(
            said.len() < 300,
            "an error page should not be quoted whole: {}",
            said.len()
        );
        assert!(said.contains("502"));
    }

    #[test]
    fn a_refusal_with_no_body_still_names_the_status() {
        let said = said_why(reqwest::StatusCode::INTERNAL_SERVER_ERROR, b"");

        assert!(said.contains("500"), "{said}");
    }

    /// The address is the one thing with no sensible default.
    #[tokio::test]
    async fn no_address_is_a_message_rather_than_a_request_to_nowhere() {
        let settings = TtsSettings::default();
        let err = speak(&settings, "hello").await.unwrap_err();

        assert!(err.contains("No address"), "{err}");
    }
}
