//! Executes the request that `providers` describes.
//!
//! Everything that can be decided without a network round trip lives in the
//! pure helpers here, so the only untested surface is the POST itself.

use crate::dictation::providers::{parse_transcript, RequestBody, TranscriptionRequest};
use crate::dictation::error::DictationError;
use std::time::Duration;

/// Whole-request ceiling. Generous because a long clip against a CPU-only
/// local server is legitimately slow, while a cloud provider answers in
/// under a second; the timeout exists to bound a hang, not to pace anyone.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(120);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// Longest provider error body worth repeating back. Gateways answer with
/// entire HTML pages, which are useless in a toast.
const MAX_REPORTED_BODY: usize = 300;

pub fn build_transcription_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .connect_timeout(CONNECT_TIMEOUT)
        .build()
        .expect("reqwest client with timeouts must build")
}

/// Sends `wav` to the endpoint `request` describes and returns the transcript.
pub async fn transcribe(
    client: &reqwest::Client,
    request: &TranscriptionRequest,
    wav: Vec<u8>,
) -> Result<String, DictationError> {
    check_upload_size(request, wav.len())?;

    let RequestBody::Multipart {
        file_field,
        filename,
        fields,
    } = &request.body;

    let mut form = reqwest::multipart::Form::new();
    for (name, value) in fields {
        form = form.text(name.clone(), value.clone());
    }
    let audio = reqwest::multipart::Part::bytes(wav)
        .file_name(filename.clone())
        // Declared explicitly: servers that sniff the container from the
        // part's content type reject the default octet-stream.
        .mime_str("audio/wav")
        .map_err(|e| DictationError::Other(format!("Could not attach the recording: {e}")))?;
    form = form.part(file_field.clone(), audio);

    let mut outgoing = client.post(&request.url).multipart(form);
    for (name, value) in &request.headers {
        outgoing = outgoing.header(name, value);
    }

    let response = outgoing
        .send()
        .await
        .map_err(|e| DictationError::Other(format!("Could not reach the transcription service: {e}")))?;

    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|e| DictationError::Other(format!("Transcription response was unreadable: {e}")))?;

    if !status.is_success() {
        return Err(describe_http_failure(status.as_u16(), &body));
    }

    parse_transcript(&body)
}

/// Rejects a clip the provider would refuse anyway.
fn check_upload_size(request: &TranscriptionRequest, len: usize) -> Result<(), DictationError> {
    match request.max_upload_bytes {
        Some(limit) if len > limit => Err(DictationError::Validation(format!(
            "That recording is too long to send: {:.1} MB exceeds the {:.0} MB limit",
            len as f64 / (1024.0 * 1024.0),
            limit as f64 / (1024.0 * 1024.0),
        ))),
        _ => Ok(()),
    }
}

/// Turns a non-2xx response into something a user can act on, preferring the
/// provider's own wording over ours.
fn describe_http_failure(status: u16, body: &str) -> DictationError {
    let reported = serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|value| {
            value
                .pointer("/error/message")
                .and_then(serde_json::Value::as_str)
                .or_else(|| value.get("error").and_then(serde_json::Value::as_str))
                .or_else(|| value.get("message").and_then(serde_json::Value::as_str))
                .map(str::to_string)
        })
        .unwrap_or_else(|| body.trim().to_string());

    let detail: String = reported.chars().take(MAX_REPORTED_BODY).collect();
    if detail.is_empty() {
        DictationError::Other(format!("Transcription failed with HTTP {status}"))
    } else {
        DictationError::Other(format!("Transcription failed (HTTP {status}): {detail}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dictation::providers::{RequestBody, TranscriptionRequest};
    use std::collections::HashMap;

    fn request(max_upload_bytes: Option<usize>) -> TranscriptionRequest {
        TranscriptionRequest {
            url: "https://example.invalid/v1/audio/transcriptions".to_string(),
            headers: HashMap::new(),
            body: RequestBody::Multipart {
                file_field: "file".to_string(),
                filename: "audio.wav".to_string(),
                fields: vec![("model".to_string(), "whisper-1".to_string())],
            },
            max_upload_bytes,
        }
    }

    // ── upload ceiling ──────────────────────────────────────────────────────

    #[test]
    fn a_clip_within_the_cap_is_accepted() {
        assert!(check_upload_size(&request(Some(10)), 10).is_ok());
    }

    #[test]
    fn a_clip_over_the_cap_is_rejected_before_it_is_sent() {
        // Rejecting locally costs nothing; discovering it after uploading
        // costs the user the whole dictation.
        let err = check_upload_size(&request(Some(10)), 11).unwrap_err();

        assert!(
            err.to_string().to_lowercase().contains("too long"),
            "the message must point at the recording, not at bytes: {err}"
        );
    }

    #[test]
    fn an_uncapped_provider_accepts_any_size() {
        assert!(check_upload_size(&request(None), 500_000_000).is_ok());
    }

    // ── failure reporting ───────────────────────────────────────────────────

    #[test]
    fn an_http_failure_surfaces_the_providers_own_message() {
        let err = describe_http_failure(401, r#"{"error":{"message":"Invalid API Key"}}"#);

        assert!(err.to_string().contains("Invalid API Key"), "{err}");
        assert!(err.to_string().contains("401"), "{err}");
    }

    #[test]
    fn an_http_failure_with_a_plain_body_still_reports_something_useful() {
        let err = describe_http_failure(502, "upstream connect error");

        assert!(err.to_string().contains("502"), "{err}");
        assert!(err.to_string().contains("upstream connect error"), "{err}");
    }

    #[test]
    fn an_enormous_error_body_is_truncated() {
        // Some gateways answer errors with a full HTML page. Pushing that into
        // a toast helps nobody.
        let err = describe_http_failure(500, &"x".repeat(10_000));

        assert!(
            err.to_string().len() < 500,
            "error was {} chars",
            err.to_string().len()
        );
    }

    #[test]
    fn an_empty_error_body_still_names_the_status() {
        let err = describe_http_failure(503, "");

        assert!(err.to_string().contains("503"), "{err}");
    }

    // ── end-to-end probe ────────────────────────────────────────────────────

    /// Records from the default microphone and transcribes it through a real
    /// provider, exercising capture, downmix, resample, WAV framing, request
    /// building, the POST, and response parsing in one pass.
    ///
    /// Ignored: it opens the microphone and spends money. Configure and run:
    ///
    /// ```text
    /// $env:ASYAR_STT_KEY   = "gsk_..."
    /// $env:ASYAR_STT_MODEL = "whisper-large-v3-turbo"
    /// cargo test --lib dictation::transcriber::tests::probe_transcribes -- --ignored --nocapture
    /// ```
    ///
    /// `ASYAR_STT_PROVIDER` (default `groq`), `ASYAR_STT_BASE_URL`,
    /// `ASYAR_STT_SECONDS` (default 5) and `ASYAR_STT_LANGUAGE` also apply.
    /// For a local whisper-server, set provider `local` and a base URL; no key
    /// or model is needed.
    #[tokio::test]
    #[ignore = "opens the microphone and calls a paid API"]
    async fn probe_transcribes_a_live_recording() {
        use crate::dictation::provider::ProviderConfig;
        use crate::dictation::capture::CaptureSession;
        use crate::dictation::providers::{transcription_request, TranscribeOptions};
        use crate::dictation::{resample, wav};
        use std::time::Instant;

        let provider = std::env::var("ASYAR_STT_PROVIDER").unwrap_or_else(|_| "groq".to_string());
        let seconds: u64 = std::env::var("ASYAR_STT_SECONDS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(5);

        let config = ProviderConfig {
            enabled: true,
            name: None,
            provider_type: None,
            api_key: std::env::var("ASYAR_STT_KEY").ok(),
            base_url: std::env::var("ASYAR_STT_BASE_URL").ok(),
            last_model_id: std::env::var("ASYAR_STT_MODEL").ok(),
        };

        let request = transcription_request(
            &provider,
            &config,
            &TranscribeOptions {
                language: std::env::var("ASYAR_STT_LANGUAGE").ok(),
                ..Default::default()
            },
        )
        .expect("build the request (set ASYAR_STT_KEY / ASYAR_STT_MODEL)");
        println!("\nPOST {}", request.url);

        println!("Recording {seconds}s. Speak now.");
        let session = CaptureSession::start(None).expect("open the microphone");
        let format = session.format();
        let capture_started = Instant::now();
        std::thread::sleep(std::time::Duration::from_secs(seconds));
        let clip = session.stop();
        println!(
            "  captured {} mono samples at {} Hz in {:.1}s (device gave {} ch)",
            clip.samples.len(),
            clip.sample_rate,
            capture_started.elapsed().as_secs_f32(),
            format.channels
        );
        assert!(
            !crate::dictation::capture::is_silent(&clip.samples),
            "the recording is silent: check the OS microphone privacy setting"
        );

        let encode_started = Instant::now();
        let samples =
            resample::to_target_rate(&clip.samples, clip.sample_rate).expect("resample to 16 kHz");
        let bytes = wav::encode_mono_16bit(&samples, resample::TARGET_RATE);
        println!(
            "  resampled to {} samples and framed {} KB in {:.0} ms",
            samples.len(),
            bytes.len() / 1024,
            encode_started.elapsed().as_secs_f32() * 1000.0
        );

        // Kept so a wrong transcript can be listened to rather than guessed at.
        let wav_path = std::env::temp_dir().join("asyar-dictation-probe.wav");
        let _ = std::fs::write(&wav_path, &bytes);
        println!("  audio written to {}", wav_path.display());

        let sent = Instant::now();
        let transcript = transcribe(&build_transcription_client(), &request, bytes)
            .await
            .expect("transcribe");
        println!(
            "  transcribed in {:.0} ms\n\n  \"{transcript}\"\n",
            sent.elapsed().as_secs_f32() * 1000.0
        );

        assert!(
            !transcript.is_empty(),
            "provider returned an empty transcript"
        );
    }
}
