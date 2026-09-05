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
    /// Not a wire format at all: the Claude Code binary on this machine.
    ///
    /// Listed here because it is a way of reaching a model and the chooser has
    /// to be able to offer it, and because it is the only one that reaches a
    /// **subscription** rather than a metered key. See `claude_code.rs`.
    ClaudeCode,
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
        id: "claudeCode",
        name: "Claude Code",
        wire: Wire::ClaudeCode,
        // Not an address. This one runs the binary already installed here.
        base_url: "",
        // Whatever the CLI is already set to.
        model: "",
        needs_key: false,
        note: "The Claude Code already on this machine, signed in as you. No key, and \n               nothing stored by Sill.",
    },
    Known {
        id: "openai",
        name: "OpenAI",
        wire: Wire::OpenAi,
        base_url: "https://api.openai.com/v1",
        model: "gpt-5.2",
        needs_key: true,
        note: "A developer console key. A ChatGPT subscription is a different thing and \n               does not pay for this.",
    },
    Known {
        id: "anthropic",
        name: "Anthropic",
        wire: Wire::Anthropic,
        base_url: "https://api.anthropic.com",
        model: "claude-sonnet-5",
        needs_key: true,
        note: "A console key. A Claude subscription cannot be used here, as their terms \n               do not allow it.",
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
        note: "A key from Google AI Studio. The free tier covers personal use.",
    },
    Known {
        id: "xai",
        name: "xAI Grok",
        wire: Wire::OpenAi,
        base_url: "https://api.x.ai/v1",
        model: "grok-4",
        needs_key: true,
        note: "A console key. SuperGrok and Premium+ are separate and cannot be used \n               here.",
    },
    Known {
        id: "openrouter",
        name: "OpenRouter",
        wire: Wire::OpenAi,
        base_url: "https://openrouter.ai/api/v1",
        model: "anthropic/claude-sonnet-5",
        needs_key: true,
        note: "One key for many models, billed in one place.",
    },
    Known {
        id: "ollama",
        name: "Ollama",
        wire: Wire::OpenAi,
        base_url: "http://localhost:11434/v1",
        // Named by the machine rather than here. What is installed differs
        // per machine, so any name shipped in this table is one most people
        // do not have, and it arrives as a picker showing nothing beside a
        // line saying there are five to choose from.
        model: "",
        needs_key: false,
        note: "A model on this machine, or one you point this at. Nothing leaves for \n               anybody else.",
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

/// The model, named as short as it can be named and still be right.
///
/// The stored id is exact and never changes: this is only what is read. It
/// matters because the id is often not a name at all.
/// `huihui_ai/qwen3-abliterated:14b` is who published it, what it is, and how
/// big, and only the last two are the answer to "which model is answering".
/// The chip has room for about twenty characters and the leading path is the
/// first thing to go.
///
/// No table of pretty names. A table mapping ids to titles is a list that goes
/// stale the week a provider ships anything, and a model whose real name is on
/// screen is never wrong in a way somebody has to discover.
pub fn short_model(wire: Wire, model: &str) -> String {
    let model = model.trim();

    // Claude Code takes aliases rather than ids, and it already ships the only
    // list of them there is. That list is maintained because the picker in
    // settings reads it, so reading it here costs nothing to keep true.
    if wire == Wire::ClaudeCode {
        // Nothing chosen answers with nothing, so whatever is reading this
        // falls back to the service name. The label for the empty alias is
        // "Whatever Claude Code is set to", which is the right words in the
        // picker that offers it and half a sentence too long anywhere else.
        if model.is_empty() {
            return String::new();
        }

        return super::claude_code::MODELS
            .iter()
            .find(|(id, _)| *id == model)
            .map(|(_, label)| (*label).to_string())
            .unwrap_or_else(|| model.to_string());
    }

    // Who published it is not which model it is. Everything up to the last
    // slash goes: `anthropic/claude-sonnet-5` is Claude Sonnet 5 whoever is
    // billing for it.
    match model.rsplit_once('/') {
        Some((_, name)) if !name.is_empty() => name.to_string(),
        _ => model.to_string(),
    }
}

/// Whether this address is this machine or this network.
///
/// The same question the rule above asks, exposed on its own because two
/// things need the answer now: where a key may be sent, and the mark on the
/// chip that says whether an answer costs anything or leaves the machine.
/// Asking here rather than matching on `localhost` at the call site is what
/// keeps one definition of local rather than two that drift.
pub fn is_on_this_network(base_url: &str) -> bool {
    strip_scheme(base_url).is_some_and(|(_, host)| is_local(host))
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
            std::net::IpAddr::V4(v4) => v4.is_loopback() || v4.is_private() || v4.is_link_local(),
            std::net::IpAddr::V6(v6) => {
                // Loopback, or a unique local address, which is IPv6's private
                // range and is every address beginning fc or fd.
                v6.is_loopback() || (v6.segments()[0] & 0xfe00) == 0xfc00
            }
        };
    }

    false
}

/// The chosen provider, or the only one if only one is set up.
///
/// Falling back to the only one is not a guess: somebody who has configured
/// exactly one provider and never opened the chooser means that one.
pub(crate) fn chosen(settings: &crate::preferences::Ai) -> Option<Provider> {
    if !settings.provider.is_empty() {
        if let Some(found) = settings
            .providers
            .iter()
            .find(|candidate| candidate.id == settings.provider)
        {
            return Some(found.clone());
        }
    }

    match settings.providers.as_slice() {
        [only] => Some(only.clone()),
        _ => None,
    }
}

/// What this provider still needs, if anything.
pub(crate) fn what_is_missing(chosen: &Provider) -> Option<String> {
    if chosen.wire == Wire::ClaudeCode {
        return crate::ai::claude_code::locate().is_none().then(|| {
            "Claude Code is not installed, or not somewhere Sill can find it.".to_string()
        });
    }

    if chosen.base_url.trim().is_empty() {
        return Some(format!("{} has no address.", chosen.name));
    }

    if let Err(refused) = check(&chosen.base_url) {
        return Some(refused.message().to_string());
    }

    if chosen.model.trim().is_empty() {
        return Some(format!("No model is chosen for {}.", chosen.name));
    }

    None
}

/// The provider that answers right now, or the sentence saying why none can.
///
/// One question asked from several places, the chat, a key bound to a text
/// action, a dictation style, so it is answered once here rather than by each
/// caller deciding what "set up" means. Here rather than in a command,
/// because an action has an app handle and no `State` extractor.
pub(crate) fn answering(settings: &crate::preferences::Ai) -> Result<Provider, String> {
    let chosen = chosen(settings)
        .ok_or_else(|| "No AI provider is set up. Add one in Settings, AI.".to_string())?;

    match what_is_missing(&chosen) {
        Some(missing) => Err(missing),
        None => Ok(chosen),
    }
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

    mod naming_the_model {
        use super::*;

        /// The ones that sent me looking for this. Both are the publisher,
        /// then the model, and only the second half answers "which model is
        /// answering".
        #[test]
        fn who_published_it_is_not_which_model_it_is() {
            assert_eq!(
                short_model(Wire::OpenAi, "huihui_ai/qwen3-abliterated:14b"),
                "qwen3-abliterated:14b",
            );
            assert_eq!(
                short_model(Wire::OpenAi, "anthropic/claude-sonnet-5"),
                "claude-sonnet-5",
            );
        }

        #[test]
        fn a_plain_name_is_left_alone() {
            assert_eq!(short_model(Wire::OpenAi, "qwen3:1.7b"), "qwen3:1.7b");
            assert_eq!(short_model(Wire::OpenAi, "gpt-5.2"), "gpt-5.2");
        }

        /// A slash with nothing after it is not a prefix, and taking the empty
        /// half would leave the chip with no model at all.
        #[test]
        fn a_trailing_slash_does_not_leave_it_nameless() {
            assert_eq!(short_model(Wire::OpenAi, "something/"), "something/");
        }

        /// Only the last one. A model published under a nested path is still
        /// named by its last segment.
        #[test]
        fn only_the_last_segment_survives() {
            assert_eq!(short_model(Wire::OpenAi, "a/b/c-model"), "c-model");
        }

        /// Claude Code takes aliases, and the list of them is already
        /// maintained because the settings picker reads it. Reading the same
        /// list here is what stops the chip saying `sonnet` while the picker
        /// beside it says Sonnet.
        #[test]
        fn an_alias_is_read_the_way_the_picker_writes_it() {
            assert_eq!(short_model(Wire::ClaudeCode, "sonnet"), "Sonnet");
            assert_eq!(short_model(Wire::ClaudeCode, "haiku"), "Haiku");
        }

        /// Empty means whatever Claude Code is set to. Its label in the
        /// picker is a whole sentence, which is the right words there and
        /// half a sentence too long in a chip, so nothing is answered and the
        /// reader falls back to the service name.
        #[test]
        fn an_alias_nobody_chose_answers_with_nothing() {
            assert_eq!(short_model(Wire::ClaudeCode, ""), "");
        }

        /// A model id Sill has never heard of still has to appear. Answering
        /// with nothing would draw an empty chip.
        #[test]
        fn an_alias_that_is_not_in_the_list_is_still_shown() {
            assert_eq!(
                short_model(Wire::ClaudeCode, "something-new"),
                "something-new"
            );
        }
    }

    mod what_ships {
        use super::*;

        /// Two adapters, and only one service needs the second.
        #[test]
        fn nearly_everything_speaks_the_same_shape() {
            let odd = KNOWN.iter().filter(|k| k.wire != Wire::OpenAi).count();

            // Anthropic's own format, and the CLI. Everything else is one
            // adapter, and a third exception should be argued for rather than
            // arrived at.
            assert_eq!(odd, 2, "something grew a wire format of its own");
        }

        /// Of the ones that are an address. Claude Code is not: it runs a
        /// binary, and a rule about where a key may be sent has nothing to
        /// say about it.
        #[test]
        fn every_shipped_address_is_one_a_key_may_be_sent_to() {
            for known in KNOWN.iter().filter(|k| k.wire != Wire::ClaudeCode) {
                assert_eq!(
                    check(known.base_url),
                    Ok(()),
                    "{} ships an address Sill would refuse",
                    known.id,
                );
            }
        }

        /// Exactly one entry is not an address, and it is the one that runs a
        /// program rather than making a request.
        #[test]
        fn the_only_one_without_an_address_is_the_one_that_runs_a_binary() {
            for known in KNOWN {
                let has_address = !known.base_url.is_empty();
                assert_eq!(
                    has_address,
                    known.wire != Wire::ClaudeCode,
                    "{} disagrees about being an address",
                    known.id,
                );
            }
        }

        /// The two that need no key: a model on this machine, and the CLI that
        /// is already signed in as you.
        #[test]
        fn a_key_is_needed_by_everything_that_reaches_somebody_elses_machine() {
            for known in KNOWN {
                let local_http =
                    !known.base_url.is_empty() && known.base_url.starts_with("http://");
                let signed_in_already = known.wire == Wire::ClaudeCode;

                assert_eq!(
                    known.needs_key,
                    !(local_http || signed_in_already),
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
            }
        }

        /// A default model, except in the two cases where naming one would be
        /// a guess at something decided elsewhere.
        ///
        /// Claude Code already has a model chosen inside Claude Code, and a
        /// name here would quietly override it. A model on somebody's own
        /// machine is whichever one they installed, so any name shipped here
        /// is one most people do not have: it arrives as a picker showing
        /// nothing beside a line saying there are five to choose from. Both
        /// are left empty and filled in from what is actually there.
        #[test]
        fn everything_whose_models_are_the_same_everywhere_names_one() {
            for known in KNOWN {
                let host = strip_scheme(known.base_url).map(|(_, host)| host);
                let decided_elsewhere =
                    known.wire == Wire::ClaudeCode || host.is_some_and(is_local);

                assert_eq!(
                    known.model.is_empty(),
                    decided_elsewhere,
                    "{} names {:?}",
                    known.id,
                    known.model,
                );
            }
        }
    }
}
