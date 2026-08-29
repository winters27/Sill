//! Recognising a credential before it is written to disk.
//!
//! The clipboard history is a plain SQLite file in `%APPDATA%`. Anything that
//! lands in it is readable by any process running as the user, and by whatever
//! backs that folder up. A password manager can say "do not record this" by
//! setting the OS exclusion formats, and the good ones do, which is already
//! honoured in `monitor::is_confidential`. **Nothing else says it.** A token
//! copied out of a terminal, a browser console, an editor or a chat message
//! arrives looking exactly like ordinary text.
//!
//! So this reads the text and decides. The whole design question is where to
//! draw the line, because the two failure modes are not symmetrical:
//!
//! - Missing a secret writes a credential to disk in the clear.
//! - **Falsely flagging ordinary text loses something the user wanted**, and
//!   they will not know why. That is the failure that makes a feature feel
//!   broken, and it is the one this errs against.
//!
//! Every detector here is therefore close to unambiguous: vendor-documented
//! prefixes, PEM blocks, and JWTs whose header really does decode to JSON.
//!
//! **Entropy scoring is deliberately not implemented.** It is the obvious next
//! detector and it is a trap: git object ids, UUIDs, content hashes, base64
//! images, minified bundles and database ids are all high entropy and all
//! completely ordinary to copy. A detector that swallows those is worse than
//! no detector, because it is unpredictable. If it is ever added it belongs
//! behind its own setting, off by default, and never behind [`Policy::Skip`].

use serde::{Deserialize, Serialize};

/// What to do with something that looks like a credential.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum Policy {
    /// Do not write it to the history at all.
    ///
    /// The default. The entry is still on the clipboard, so nothing the user
    /// was doing is interrupted; it simply leaves no trace on disk.
    #[default]
    Skip,
    /// Record that something was copied, with the value replaced.
    ///
    /// For someone who wants the history to be complete enough to reason
    /// about without holding the secret itself.
    Redact,
    /// Record it like anything else.
    Keep,
}

/// What a piece of copied text appears to be.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sensitivity {
    /// Nothing suggests this is a credential.
    Ordinary,
    /// Recognised, with the name of what it looks like.
    ///
    /// The name is shown to the user, so it is the vendor's word for the
    /// thing rather than an internal label: somebody who sees "GitHub token"
    /// can tell instantly whether the guess was right.
    Secret(&'static str),
}

impl Sensitivity {
    pub fn secret(self) -> Option<&'static str> {
        match self {
            Sensitivity::Secret(kind) => Some(kind),
            Sensitivity::Ordinary => None,
        }
    }
}

/// Prefixes that identify a credential on their own.
///
/// Every one of these is documented by its vendor as the marker for a
/// credential, which is exactly why they exist: they are what secret scanners
/// match on. Nothing else in ordinary text begins with them.
///
/// The length floor matters as much as the prefix. `sk-` on its own is three
/// characters and appears inside ordinary words; `sk-` followed by forty more
/// characters of key alphabet does not.
const PREFIXES: &[(&str, usize, &str)] = &[
    // GitHub, which publishes these exact prefixes for scanning.
    ("ghp_", 36, "GitHub personal access token"),
    ("gho_", 36, "GitHub OAuth token"),
    ("ghu_", 36, "GitHub user-to-server token"),
    ("ghs_", 36, "GitHub server-to-server token"),
    ("ghr_", 36, "GitHub refresh token"),
    ("github_pat_", 40, "GitHub fine-grained token"),
    // OpenAI and Anthropic. The Anthropic form is checked first below because
    // it also begins with `sk-`.
    ("sk-ant-", 40, "Anthropic API key"),
    ("sk-proj-", 40, "OpenAI project key"),
    ("sk-", 40, "OpenAI API key"),
    // Slack.
    ("xoxb-", 24, "Slack bot token"),
    ("xoxp-", 24, "Slack user token"),
    ("xoxa-", 24, "Slack app token"),
    ("xoxs-", 24, "Slack token"),
    ("xapp-", 24, "Slack app-level token"),
    // Cloud and platform.
    ("AKIA", 20, "AWS access key id"),
    ("ASIA", 20, "AWS temporary access key id"),
    ("AIza", 35, "Google API key"),
    ("ya29.", 30, "Google OAuth token"),
    ("glpat-", 20, "GitLab personal access token"),
    ("dop_v1_", 40, "DigitalOcean token"),
    ("npm_", 36, "npm access token"),
    ("SG.", 40, "SendGrid API key"),
    ("shpat_", 32, "Shopify access token"),
    ("shpss_", 32, "Shopify shared secret"),
    ("hf_", 34, "Hugging Face token"),
    // Stripe. The live keys only; test keys are published in documentation and
    // pasted around constantly, and flagging those would be pure noise.
    ("sk_live_", 24, "Stripe secret key"),
    ("rk_live_", 24, "Stripe restricted key"),
];

/// Names that mean a credential when something is assigned to them.
///
/// Kept narrow. `key` alone is not here, because "key" means a map key, a
/// sort key and a keyboard key far more often than it means a secret.
const ASSIGNED: &[&str] = &[
    "password",
    "passwd",
    "api_key",
    "apikey",
    "api-key",
    "secret",
    "secret_key",
    "client_secret",
    "access_token",
    "auth_token",
    "private_key",
];

/// Values that are obviously not the secret itself.
///
/// Documentation, templates and `.env.example` files are full of these, and
/// they are copied constantly. Flagging them would make the feature feel
/// random for the people most likely to notice.
fn is_placeholder(value: &str) -> bool {
    let lowered = value.trim().to_lowercase();

    if lowered.len() < 8 {
        return true;
    }

    // A run of one character: xxxxxxxx, ********, 00000000.
    if lowered
        .chars()
        .all(|c| c == lowered.chars().next().unwrap_or(' '))
    {
        return true;
    }

    const OBVIOUS: &[&str] = &[
        "your",
        "example",
        "changeme",
        "placeholder",
        "todo",
        "xxxx",
        "....",
        "<",
        "insert",
        "replace",
        "redacted",
        "hidden",
        "secret_here",
        "dummy",
        "sample",
        "test",
    ];

    OBVIOUS.iter().any(|word| lowered.contains(word))
}

/// Whether an assigned value could be a credential rather than code.
///
/// **This is what stops `let password = read_line()?;` being flagged**, which
/// is the most likely false positive in a developer's clipboard and exactly
/// the failure this module is written to avoid. A credential is one run of
/// credential characters. Anything with brackets, semicolons, spaces or
/// operators in it is an expression that mentions a password, not a password.
fn looks_like_a_value(value: &str) -> bool {
    !value.is_empty()
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_.+/=~:@".contains(c))
}

/// Whether the text looks like a credential.
///
/// Cheap. It runs on every copy, so it is a handful of prefix comparisons over
/// the first token and a scan for a PEM header, not a regex engine.
pub fn classify(text: &str) -> Sensitivity {
    let trimmed = text.trim();

    if trimmed.is_empty() {
        return Sensitivity::Ordinary;
    }

    // A private key, which is unmistakable and the worst thing on this list to
    // leave lying in a database. The specific form first: an OpenSSH key also
    // matches the general shape, and reporting it as a generic "private key"
    // is less use to somebody deciding whether the guess was right.
    if trimmed.contains("-----BEGIN OPENSSH PRIVATE KEY-----") {
        return Sensitivity::Secret("SSH private key");
    }

    if trimmed.contains("-----BEGIN") && trimmed.contains("PRIVATE KEY-----") {
        return Sensitivity::Secret("private key");
    }

    // Single-token forms. A pasted credential is one word; a paragraph that
    // happens to contain one is prose about a credential, and treating a whole
    // document as a secret because of one substring is how a detector starts
    // eating things.
    let word_count = trimmed.split_whitespace().take(2).count();
    if word_count == 1 {
        let token = trimmed;

        for (prefix, least, kind) in PREFIXES {
            if token.starts_with(prefix) && token.len() >= *least {
                return Sensitivity::Secret(kind);
            }
        }

        if is_jwt(token) {
            return Sensitivity::Secret("JSON web token");
        }
    }

    // `PASSWORD=hunter2`, `api_key: abcd...`. Checked over the whole text
    // rather than a single token, because this is the form a `.env` file and a
    // pasted config take, and those are the most common way a secret reaches
    // the clipboard by accident.
    if let Some(kind) = assigned_secret(trimmed) {
        return Sensitivity::Secret(kind);
    }

    Sensitivity::Ordinary
}

/// A JWT: three base64url parts, whose header really is a JSON object with an
/// algorithm in it.
///
/// The shape alone is not enough. `a.b.c` and plenty of dotted identifiers
/// match "three parts separated by dots"; decoding the header is what makes
/// this a near-certain match rather than a guess.
fn is_jwt(token: &str) -> bool {
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 {
        return false;
    }

    if parts.iter().any(|part| part.is_empty()) {
        return false;
    }

    // Signature and payload have to at least look like base64url.
    if !parts.iter().all(|part| {
        part.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '=')
    }) {
        return false;
    }

    let Ok(header) = crate::text::base64_decode(parts[0]) else {
        return false;
    };

    let Ok(value) = serde_json::from_str::<serde_json::Value>(&header) else {
        return false;
    };

    value.get("alg").is_some()
}

/// `name = value` where the name means a credential and the value is real.
fn assigned_secret(text: &str) -> Option<&'static str> {
    // Only the first few lines. A whole file pasted in is not a secret because
    // line ninety happens to set a password, and reading all of it on every
    // copy is work nobody asked for.
    for line in text.lines().take(8) {
        let lowered = line.to_lowercase();

        for name in ASSIGNED {
            // Followed by `=` or `:`, with optional spaces, which covers env
            // files, INI, YAML, JSON and connection strings alike.
            let Some(at) = lowered.find(name) else {
                continue;
            };

            // The name cannot be the tail of a longer word: `mypassword` is
            // somebody's variable, not an assignment of `password`. A
            // separator before it is fine and is in fact the normal case,
            // because environment variables are prefixed: `DATABASE_PASSWORD`,
            // `MYAPP_API_KEY`. What stops `passwordless=true` matching is the
            // check on what comes *after* the name, not this one.
            let before_ok = at == 0 || !lowered.as_bytes()[at - 1].is_ascii_alphanumeric();
            if !before_ok {
                continue;
            }

            let rest = &line[at + name.len()..];
            let rest = rest.trim_start();
            let Some(value) = rest
                .strip_prefix('=')
                .or_else(|| rest.strip_prefix(':'))
                .or_else(|| rest.strip_prefix("\":"))
                .or_else(|| rest.strip_prefix("\"="))
            else {
                continue;
            };

            let value = value.trim().trim_matches(['"', '\'', ','].as_slice());
            if is_placeholder(value) || !looks_like_a_value(value) {
                continue;
            }

            return Some("credential in a configuration line");
        }
    }

    None
}

/// What goes in the history in place of a secret.
pub fn redacted(kind: &str, length: usize) -> String {
    format!("[{kind} not stored, {length} characters]")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn secret(text: &str) -> Option<&'static str> {
        classify(text).secret()
    }

    #[test]
    fn vendor_prefixes_are_recognised() {
        // Every one of these is a documented scanning prefix. Missing one
        // writes a live credential to a plain file on disk.
        let cases = [
            (
                "ghp_16C7e42F292c6912E7710c838347Ae178B4a",
                "GitHub personal access token",
            ),
            (
                "github_pat_11ABCDEFG0abcdefghijkl_ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789abcd",
                "GitHub fine-grained token",
            ),
            (
                "xoxb-123456789012-1234567890123-AbCdEfGhIjKlMnOpQrStUvWx",
                "Slack bot token",
            ),
            ("AKIAIOSFODNN7EXAMPLE", "AWS access key id"),
            ("AIzaSyD-1234567890abcdefghijklmnopqrstuv", "Google API key"),
            ("glpat-ABCDEFGHIJKLMNOPQRST", "GitLab personal access token"),
            (
                "npm_abcdefghijklmnopqrstuvwxyz0123456789",
                "npm access token",
            ),
            ("sk_live_abcdefghijklmnopqrstuvwx", "Stripe secret key"),
            (
                "hf_abcdefghijklmnopqrstuvwxyz012345678",
                "Hugging Face token",
            ),
        ];

        for (token, expected) in cases {
            assert_eq!(secret(token), Some(expected), "{token}");
        }
    }

    #[test]
    fn anthropic_is_not_reported_as_openai() {
        // Both begin `sk-`, so the order of the table is load-bearing. Getting
        // this wrong tells the user the wrong vendor, which is the one thing
        // that would make them distrust the whole feature.
        let key = "sk-ant-api03-abcdefghijklmnopqrstuvwxyz0123456789";
        assert_eq!(secret(key), Some("Anthropic API key"));

        let openai = "sk-abcdefghijklmnopqrstuvwxyz0123456789ABCD";
        assert_eq!(secret(openai), Some("OpenAI API key"));
    }

    #[test]
    fn a_prefix_without_the_length_is_not_a_secret() {
        // "sk-" is three characters and appears inside ordinary text. The
        // length floor is what makes the prefix usable at all.
        assert_eq!(secret("sk-"), None);
        assert_eq!(secret("ghp_short"), None);
        assert_eq!(secret("AKIA"), None);
    }

    #[test]
    fn private_keys_of_every_kind_are_caught() {
        for header in [
            "-----BEGIN RSA PRIVATE KEY-----",
            "-----BEGIN PRIVATE KEY-----",
            "-----BEGIN EC PRIVATE KEY-----",
        ] {
            let pem = format!("{header}\nMIIEpAIBAAKCAQEA...\n-----END PRIVATE KEY-----");
            assert!(secret(&pem).is_some(), "{header}");
        }

        let ssh = "-----BEGIN OPENSSH PRIVATE KEY-----\nb3BlbnNzaC1rZXktdjEA\n-----END OPENSSH PRIVATE KEY-----";
        assert_eq!(secret(ssh), Some("SSH private key"));
    }

    #[test]
    fn a_public_key_is_not_a_private_one() {
        // Public keys are pasted constantly and are not secret.
        let public = "-----BEGIN PUBLIC KEY-----\nMIIBIjANBgkq\n-----END PUBLIC KEY-----";
        assert_eq!(secret(public), None);

        let ssh_public = "ssh-rsa AAAAB3NzaC1yc2EAAAADAQABAAABgQC brandon@desk";
        assert_eq!(secret(ssh_public), None);
    }

    #[test]
    fn a_real_jwt_is_caught_and_a_dotted_name_is_not() {
        // Header {"alg":"HS256","typ":"JWT"} then a payload and signature.
        let jwt = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.dBjftJeZ4CVPmB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        assert_eq!(secret(jwt), Some("JSON web token"));

        // Same shape, no decodable header. These are everywhere: package
        // names, hostnames, version strings, object paths.
        for ordinary in [
            "com.example.app",
            "a.b.c",
            "src.lib.main",
            "1.2.3",
            "www.github.com",
        ] {
            assert_eq!(secret(ordinary), None, "{ordinary}");
        }
    }

    #[test]
    fn a_configuration_line_with_a_real_value_is_caught() {
        for line in [
            "PASSWORD=s3cr3t-actual-value-here",
            "api_key = 8f14e45fceea167a5a36dedd4bea2543",
            "client_secret: GOCSPX-abcdefghijklmnop",
            "DATABASE_PASSWORD=hunter2hunter2hunter2",
        ] {
            assert!(secret(line).is_some(), "{line}");
        }
    }

    #[test]
    fn documentation_and_templates_are_left_alone() {
        // The failure that matters. Someone copying an example out of a readme
        // and finding it silently missing from their history has no way to
        // know why, and will conclude the history is unreliable.
        for line in [
            "PASSWORD=your-password-here",
            "api_key=<YOUR_API_KEY>",
            "SECRET=changeme",
            "password=xxxxxxxxxx",
            "client_secret: example",
            "PASSWORD=********",
            "api_key=TODO",
            "password=",
        ] {
            assert_eq!(secret(line), None, "{line}");
        }
    }

    #[test]
    fn a_word_that_merely_contains_a_secret_name_is_not_an_assignment() {
        for line in [
            "passwordless=true",
            "my_password_hint = the usual one",
            "secretariat = booked",
        ] {
            assert_eq!(secret(line), None, "{line}");
        }
    }

    #[test]
    fn ordinary_things_people_copy_are_never_flagged() {
        // The false-positive set, and the reason entropy scoring is not here.
        // Every one of these is high entropy and completely ordinary.
        for text in [
            // Git object ids and content hashes.
            "6143c4e8f2a1b9d0c3e4f5a6b7c8d9e0f1a2b3c4",
            "sha256:9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08",
            // A UUID, a database id, a session id.
            "550e8400-e29b-41d4-a716-446655440000",
            "01HQ8Z9K3M4N5P6Q7R8S9T0V1W",
            // Base64 that is not a credential.
            "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==",
            // Ordinary prose, code and paths.
            "The api_key parameter is required.",
            "let password = read_line()?;",
            "const apiKey = process.env.API_KEY;",
            "password: await bcrypt.hash(raw, 12),",
            "secret = generate_secret()",
            "C:\\\\Users\\\\Brandon\\\\Documents",
            "https://github.com/winters27/Sill",
            "npm install --save-dev vitest",
            // A long random-looking string with no marker at all. Without a
            // prefix there is nothing to distinguish this from an id.
            "aB3dE5fG7hJ9kL1mN3pQ5rS7tU9vW1xY3z",
        ] {
            assert_eq!(secret(text), None, "flagged: {text}");
        }
    }

    #[test]
    fn prose_about_a_token_is_not_a_token() {
        // A whole paragraph is not a credential because it mentions one. This
        // is what the single-word rule buys.
        let message =
            "I regenerated the key, the old ghp_16C7e42F292c6912E7710c838347Ae178B4a is dead now";
        assert_eq!(secret(message), None);
    }

    #[test]
    fn the_redaction_says_what_was_skipped_and_nothing_more() {
        // It goes in the history, so it must carry no part of the value.
        let line = redacted("GitHub personal access token", 40);
        assert!(line.contains("GitHub personal access token"));
        assert!(line.contains("40"));
        assert!(!line.contains("ghp_"));
    }

    #[test]
    fn the_default_policy_does_not_write_the_secret_down() {
        // The whole point. A default of Keep would make this feature a
        // decoration.
        assert_eq!(Policy::default(), Policy::Skip);
    }
}
