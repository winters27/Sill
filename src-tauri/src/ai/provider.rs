//! Who answers, and how to reach them.
//!
//! ## Why there are only two wire formats
//!
//! Nearly everything speaks OpenAI's shape now, including the ones that are
//! not OpenAI: xAI, Ollama, OpenRouter, LM Studio, and Google's own
//! compatibility endpoint. Anthropic is the exception worth writing a second
//! adapter for. So one format covers six services and a custom endpoint, and
//! the second covers one.
//!
//! That is a fact about the world in 2026 rather than a design, and it is
//! written down because the temptation is to build an abstraction per vendor
//! and end up maintaining six copies of the same request.
//!
//! ## What a key is, and is not
//!
//! Every one of these is reached with an **API key**, entered by the person
//! using it. None of them is reached by signing into a chat subscription:
//!
//! - Anthropic prohibits it outright. Using OAuth tokens from a Free, Pro or
//!   Max account in another product is against their terms, and they have
//!   banned accounts for it.
//! - OpenAI's "Sign in with ChatGPT" grants identity and credits against the
//!   developer's own billing, not model usage on the person's ChatGPT plan.
//! - xAI does have a subscription OAuth flow, and it is behind an allowlist
//!   they do not publish. Worth asking them about; not worth guessing at.
//! - Google discontinued the free consumer login for its own CLI in 2026.
//!
//! Keys are sealed with DPAPI before they touch the preferences file. See
//! `secrets.rs`.

use serde::{Deserialize, Serialize};

/// The shape of the conversation a service expects.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum Wire {
    /// Chat completions, as OpenAI defined them and everyone else copied.
    #[default]
    OpenAi,
    /// Anthropic's messages API, which is its own shape.
    Anthropic,
}

/// One service, as configured.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Provider {
    /// Stable across renames, because the chosen provider is stored by it.
    pub id: String,
    /// What to call it.
    pub name: String,
    pub wire: Wire,
    /// Where to send the request.
    pub base_url: String,
    /// Sealed on disk. Empty for anything that needs no key, such as a model
    /// running on this machine.
    pub api_key: String,
    /// Which model to ask, of the ones this service has.
    pub model: String,
}

impl Default for Provider {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            wire: Wire::OpenAi,
            base_url: String::new(),
            api_key: String::new(),
            model: String::new(),
        }
    }
}

/// A service somebody can set up without typing a URL.
pub struct Known {
    pub id: &'static str,
    pub name: &'static str,
    pub wire: Wire,
    pub base_url: &'static str,
    /// A sensible default, so a new provider works before anybody picks.
    pub model: &'static str,
    /// Whether a key is needed at all.
    pub needs_key: bool,
    /// One line about what setting it up involves.
    pub note: &'static str,
}

/// The services Sill offers out of the box.
///
/// Every one of them but Anthropic speaks the OpenAI shape, including Google's
/// and xAI's own endpoints. A custom entry covers anything not listed, which
/// in practice is another OpenAI-compatible gateway.
pub const KNOWN: &[Known] = &[
    Known {
        id: "openai",
        name: "OpenAI",
        wire: Wire::OpenAi,
        base_url: "https://api.openai.com/v1",
        model: "gpt-5.2",
        needs_key: true,
        note: "A key from the OpenAI developer console. A ChatGPT subscription is \
               a different thing and does not pay for this.",
    },
    Known {
        id: "anthropic",
        name: "Anthropic",
        wire: Wire::Anthropic,
        base_url: "https://api.anthropic.com",
        model: "claude-sonnet-5",
        needs_key: true,
        note: "A key from the Anthropic console. A Claude subscription cannot be \
               used here: their terms do not allow it in other applications.",
    },
    Known {
        id: "google",
        name: "Google Gemini",
        wire: Wire::OpenAi,
        // Google publishes an OpenAI-compatible route beside its own, which
        // means one adapter rather than a third.
        base_url: "https://generativelanguage.googleapis.com/v1beta/openai",
        model: "gemini-3-flash",
        needs_key: true,
        note: "A key from Google AI Studio. There is a free tier that is enough \
               for personal use.",
    },
    Known {
        id: "xai",
        name: "xAI Grok",
        wire: Wire::OpenAi,
        base_url: "https://api.x.ai/v1",
        model: "grok-4",
        needs_key: true,
        note: "A key from the xAI console. A SuperGrok or Premium+ subscription \
               is separate and cannot be used here.",
    },
    Known {
        id: "openrouter",
        name: "OpenRouter",
        wire: Wire::OpenAi,
        base_url: "https://openrouter.ai/api/v1",
        model: "anthropic/claude-sonnet-5",
        needs_key: true,
        note: "One key for many models, billed by them rather than by each \
               service separately.",
    },
    Known {
        id: "ollama",
        name: "Ollama",
        wire: Wire::OpenAi,
        base_url: "http://localhost:11434/v1",
        model: "llama4",
        needs_key: false,
        note: "A model running on this machine, or on another one you point \
               this at. Nothing leaves for anybody else.",
    },
];

impl Known {
    /// This service, as a provider ready to be saved.
    pub fn provider(&self) -> Provider {
        Provider {
            id: self.id.to_string(),
            name: self.name.to_string(),
            wire: self.wire,
            base_url: self.base_url.to_string(),
            api_key: String::new(),
            model: self.model.to_string(),
        }
    }
}

/// Why an address cannot be used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refused {
    Empty,
    /// Not a scheme this can speak.
    NotHttp,
    /// Plain http to somewhere that is not this machine or this network.
    ///
    /// The request carries an API key and the whole conversation. Over plain
    /// http both are readable by anything between here and there.
    InsecureAndRemote,
}

impl Refused {
    pub fn message(self) -> &'static str {
        match self {
            Self::Empty => "that provider has no address",
            Self::NotHttp => "an address has to start with http:// or https://",
            Self::InsecureAndRemote => {
                "http is only allowed to this machine or this network. \
                 Anywhere else needs https, or the key and the whole \
                 conversation travel in the clear"
            }
        }
    }
}

/// Whether an address is one Sill will send a key to.
///
/// **Plain http is allowed to this machine and this network, and nowhere
/// else.** A model running on localhost has no certificate and never will,
/// and refusing that would rule out the entire reason somebody runs one. A
/// plain http address on the open internet is a different thing: the key and
/// every word of the conversation are readable by anything on the path, and
/// nobody types such an address on purpose.
///
/// A rule rather than a checkbox, deliberately. A checkbox marked "allow
/// insecure" is one people tick to make an error go away.
pub fn check(base_url: &str) -> Result<(), Refused> {
    let url = base_url.trim();

    if url.is_empty() {
        return Err(Refused::Empty);
    }

    let Some(rest) = strip_scheme(url) else {
        return Err(Refused::NotHttp);
    };

    let (scheme, host) = rest;

    if scheme == Scheme::Https {
        return Ok(());
    }

    if is_local(host) {
        return Ok(());
    }

    Err(Refused::InsecureAndRemote)
}

#[derive(PartialEq, Eq)]
enum Scheme {
    Http,
    Https,
}

/// The scheme, and the host that follows it.
fn strip_scheme(url: &str) -> Option<(Scheme, &str)> {
    let lower = url.to_ascii_lowercase();

    let (scheme, at) = if lower.starts_with("https://") {
        (Scheme::Https, "https://".len())
    } else if lower.starts_with("http://") {
        (Scheme::Http, "http://".len())
    } else {
        return None;
    };

    let rest = &url[at..];

    // Up to the first slash, and without any credentials or port.
    let host = rest
        .split('/')
        .next()
        .unwrap_or(rest)
        .rsplit('@')
        .next()
        .unwrap_or("");

    let host = match host.rfind(':') {
        // A port, unless the colon belongs to a bracketed IPv6 address.
        Some(at) if !host.ends_with(']') => &host[..at],
        _ => host,
    };

    Some((scheme, host.trim_matches(['[', ']'])))
}

/// Whether a host is this machine or something on this network.
///
/// The private ranges as well as loopback, because somebody running a model on
/// the desktop under their desk and reaching it from a laptop is doing the same
/// thing as running it here: the traffic does not leave their network, and
/// there is no certificate authority that would issue for `192.168.1.9`.
fn is_local(host: &str) -> bool {
    let host = host.to_ascii_lowercase();

    if host == "localhost" || host.ends_with(".localhost") || host.ends_with(".local") {
        return true;
    }

    if let Ok(address) = host.parse::<std::net::IpAddr>() {
        return match address {
            std::net::IpAddr::V4(v4) => {
                v4.is_loopback() || v4.is_private() || v4.is_link_local()
            }
            std::net::IpAddr::V6(v6) => {
                // Loopback, or a unique local address, which is IPv6's private
                // range and is every address beginning fc or fd.
                v6.is_loopback() || (v6.segments()[0] & 0xfe00) == 0xfc00
            }
        };
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    mod where_a_key_may_be_sent {
        use super::*;

        #[test]
        fn https_anywhere_is_fine() {
            for url in [
                "https://api.openai.com/v1",
                "https://api.anthropic.com",
                "https://models.example.com:8443/v1",
            ] {
                assert_eq!(check(url), Ok(()), "{url} was refused");
            }
        }

        /// A model on this machine has no certificate and never will, and
        /// refusing it would rule out the entire reason somebody runs one.
        #[test]
        fn http_to_this_machine_is_fine() {
            for url in [
                "http://localhost:11434/v1",
                "http://127.0.0.1:11434/v1",
                "http://[::1]:11434/v1",
                "http://ollama.localhost/v1",
            ] {
                assert_eq!(check(url), Ok(()), "{url} was refused");
            }
        }

        /// A model on the desktop under the desk, reached from a laptop, is
        /// the same thing: the traffic does not leave the network and no
        /// authority would issue a certificate for that address.
        #[test]
        fn http_to_this_network_is_fine() {
            for url in [
                "http://192.168.1.9:11434/v1",
                "http://10.0.0.4:11434/v1",
                "http://172.16.3.2/v1",
                "http://[fd00::1]:11434/v1",
            ] {
                assert_eq!(check(url), Ok(()), "{url} was refused");
            }
        }

        /// The key and every word of the conversation would be readable by
        /// anything on the path, and nobody types such an address on purpose.
        #[test]
        fn http_to_the_open_internet_is_refused() {
            for url in [
                "http://api.openai.com/v1",
                "http://example.com/v1",
                "http://8.8.8.8/v1",
                "http://172.32.0.1/v1",
            ] {
                assert_eq!(
                    check(url),
                    Err(Refused::InsecureAndRemote),
                    "{url} was allowed",
                );
            }
        }

        /// A host that only looks private. `172.16` to `172.31` is the private
        /// range; `172.32` is somebody else's.
        #[test]
        fn the_edges_of_the_private_range_are_where_they_should_be() {
            assert_eq!(check("http://172.31.255.254/v1"), Ok(()));
            assert_eq!(
                check("http://172.32.0.1/v1"),
                Err(Refused::InsecureAndRemote),
            );
            assert_eq!(check("http://10.255.255.255/v1"), Ok(()));
            assert_eq!(check("http://11.0.0.1/v1"), Err(Refused::InsecureAndRemote));
        }

        /// Credentials in the address must not be mistaken for the host.
        #[test]
        fn a_username_in_the_address_does_not_disguise_the_host() {
            assert_eq!(
                check("http://localhost@evil.example.com/v1"),
                Err(Refused::InsecureAndRemote),
            );
        }

        #[test]
        fn something_that_is_not_an_address_is_refused() {
            assert_eq!(check(""), Err(Refused::Empty));
            assert_eq!(check("   "), Err(Refused::Empty));
            assert_eq!(check("ftp://example.com"), Err(Refused::NotHttp));
            assert_eq!(check("api.openai.com/v1"), Err(Refused::NotHttp));
        }

        #[test]
        fn the_scheme_is_read_whatever_its_case() {
            assert_eq!(check("HTTPS://api.example.com/v1"), Ok(()));
            assert_eq!(check("HTTP://LOCALHOST:11434/v1"), Ok(()));
        }
    }

    mod what_ships {
        use super::*;

        /// Two adapters, and only one service needs the second.
        #[test]
        fn all_but_one_speak_the_same_shape() {
            let anthropic = KNOWN.iter().filter(|k| k.wire == Wire::Anthropic).count();
            assert_eq!(anthropic, 1, "a second wire format grew a second user");
        }

        #[test]
        fn every_shipped_address_is_one_a_key_may_be_sent_to() {
            for known in KNOWN {
                assert_eq!(
                    check(known.base_url),
                    Ok(()),
                    "{} ships an address Sill would refuse",
                    known.id,
                );
            }
        }

        /// A model on this machine is the one that needs no key.
        #[test]
        fn only_the_local_one_needs_no_key() {
            for known in KNOWN {
                let local = check(known.base_url).is_ok() && known.base_url.starts_with("http://");
                assert_eq!(
                    known.needs_key, !local,
                    "{} disagrees about needing a key",
                    known.id,
                );
            }
        }

        #[test]
        fn none_of_them_share_an_id() {
            let mut seen = std::collections::HashSet::new();
            for known in KNOWN {
                assert!(seen.insert(known.id), "{} is listed twice", known.id);
            }
        }

        /// Every one of them says what setting it up involves, because three
        /// of the six have a subscription with the same name that does not
        /// pay for this.
        #[test]
        fn every_one_of_them_explains_itself() {
            for known in KNOWN {
                assert!(!known.note.is_empty(), "{} says nothing", known.id);
                assert!(!known.model.is_empty(), "{} has no default model", known.id);
            }
        }
    }
}
