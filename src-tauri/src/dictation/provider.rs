//! How a transcription backend is addressed.
//!
//! Sill has no AI chat module, so this carries only what transcription needs:
//! where to send the audio, what to authenticate with, and which model to ask
//! for. Everything else a chat provider would want (temperature, token caps,
//! reasoning effort) has no meaning for a speech-to-text POST.

use serde::{Deserialize, Serialize};

/// One configured transcription backend.
///
/// Every field is optional because the provider decides which it needs: the
/// local whisper server wants a base URL and no key, OpenAI wants a key and
/// no base URL, and a self-hosted OpenAI-compatible endpoint wants both.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ProviderConfig {
    /// Whether the user has finished setting this one up.
    pub enabled: bool,
    /// What to call it in the UI, when it is a custom endpoint.
    pub name: Option<String>,
    /// Which built-in shape it speaks, for custom endpoints.
    pub provider_type: Option<String>,
    pub api_key: Option<String>,
    pub base_url: Option<String>,
    /// The model last chosen for this provider.
    pub last_model_id: Option<String>,
}

/// Appends the OpenAI version segment to a base URL unless it is already there.
///
/// Users paste the base URL in whatever form their provider's documentation
/// showed them, which is `https://host` for some and `https://host/v1` for
/// others. Appending unconditionally produces `/v1/v1/audio/transcriptions`,
/// which 404s with nothing in the message explaining why.
///
/// Anything that already ends in a version segment is left alone, and so is
/// a URL ending in `openai`, which is how Azure and several gateways lay out
/// their OpenAI-compatible route.
pub fn normalize_openai_base(base: &str) -> String {
    let trimmed = base.trim_end_matches('/');

    let last = trimmed.rsplit('/').next().unwrap_or_default();
    let is_version = last
        .strip_prefix('v')
        .is_some_and(|digits| !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()));

    if is_version || last.eq_ignore_ascii_case("openai") {
        trimmed.to_string()
    } else {
        format!("{trimmed}/v1")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bare_host_gains_the_version_segment() {
        assert_eq!(
            normalize_openai_base("https://api.openai.com"),
            "https://api.openai.com/v1"
        );
    }

    #[test]
    fn a_url_that_already_has_one_is_left_alone() {
        // The whole point: pasting the documented URL must not double it.
        assert_eq!(
            normalize_openai_base("https://api.groq.com/openai/v1"),
            "https://api.groq.com/openai/v1"
        );
        assert_eq!(
            normalize_openai_base("https://openrouter.ai/api/v1/"),
            "https://openrouter.ai/api/v1"
        );
    }

    #[test]
    fn a_multi_digit_version_counts_as_a_version() {
        assert_eq!(
            normalize_openai_base("https://host/v10"),
            "https://host/v10"
        );
    }

    #[test]
    fn an_openai_suffix_is_a_route_not_a_host() {
        // Azure and several gateways end their compatible route at `openai`.
        assert_eq!(
            normalize_openai_base("https://example.azure.com/openai"),
            "https://example.azure.com/openai"
        );
    }

    #[test]
    fn a_word_starting_with_v_is_not_a_version() {
        // "voice" begins with v but is a path segment, so the version is
        // still missing and has to be added.
        assert_eq!(
            normalize_openai_base("https://host/voice"),
            "https://host/voice/v1"
        );
    }

    #[test]
    fn a_trailing_slash_never_survives() {
        assert_eq!(normalize_openai_base("https://host/"), "https://host/v1");
    }
}
