//! What the window can ask of the AI side.

use tauri::{AppHandle, State};

use tauri::Manager;

use crate::ai::chat::{Chat, Turn};
use crate::ai::provider::{self, Provider};
use crate::state::PrefsState;

/// Who is set up to answer, and who is chosen.
///
/// Read by the chip at the end of the search field, which is the only place
/// anybody ever discovers that Tab does anything. So it carries enough to draw
/// that chip without a second call: the name, the model, and which of the
/// three kinds of provider it is.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Ready {
    /// Whether anything is set up at all.
    pub ready: bool,
    /// Which provider, so the window can draw its mark.
    pub id: String,
    /// What the chosen one is called, for a line saying who answered.
    pub name: String,
    /// The model as it is read, which is not the id it is asked for.
    ///
    /// Short on purpose: the mark beside it already says which service, so
    /// repeating that here would spend the width twice.
    pub model: String,
    /// `local`, `cli` or `key`. What the mark on the chip is drawn from.
    ///
    /// Three kinds rather than seven names, because the useful distinction is
    /// where the answer comes from and who pays for it: this machine, a
    /// subscription through a tool already signed in, or a key.
    pub kind: String,
    /// Why not, when not. Empty when it is ready.
    pub why_not: String,
}

/// Which of the three kinds of provider this is.
fn kind_of(provider: &Provider) -> String {
    if provider.wire == provider::Wire::ClaudeCode {
        return "cli".to_string();
    }

    // Local means the machine answers, whoever owns it. The address rule
    // already knows how to tell, and asking it here means one definition of
    // "local" rather than two that drift.
    if provider::is_on_this_network(&provider.base_url) {
        return "local".to_string();
    }

    "key".to_string()
}

/// Whether asking would work, and who would answer.
///
/// Asked before offering the question rather than after: a launcher that
/// invites you to press Tab and then says "no provider" has wasted the
/// keystroke and the sentence you typed.
#[tauri::command]
pub(crate) async fn ai_ready(prefs: State<'_, PrefsState>) -> Result<Ready, String> {
    let settings = prefs.inner.lock().await.ai.clone();

    let Some(chosen) = chosen(&settings) else {
        return Ok(Ready {
            ready: false,
            id: String::new(),
            name: String::new(),
            model: String::new(),
            kind: String::new(),
            why_not: if settings.providers.is_empty() {
                "Nothing is set up to answer yet.".to_string()
            } else {
                "No provider is chosen.".to_string()
            },
        });
    };

    // A provider missing the one thing it needs is not ready, and saying which
    // thing beats a request that fails with somebody else's error message.
    if let Some(missing) = what_is_missing(&chosen) {
        return Ok(Ready {
            ready: false,
            id: chosen.id.clone(),
            name: chosen.name.clone(),
            model: provider::short_model(chosen.wire, &chosen.model),
            kind: kind_of(&chosen),
            why_not: missing,
        });
    }

    Ok(Ready {
        ready: true,
        id: chosen.id.clone(),
        name: chosen.name.clone(),
        model: provider::short_model(chosen.wire, &chosen.model),
        kind: kind_of(&chosen),
        why_not: String::new(),
    })
}

/// The conversation so far, for a window that has just opened.
#[tauri::command]
pub(crate) fn ai_transcript(chat: State<'_, Chat>) -> Vec<Turn> {
    chat.transcript()
}

/// Forgets the conversation.
#[tauri::command]
pub(crate) fn ai_clear(chat: State<'_, Chat>) {
    chat.clear();
}

/// Asks the first question of a new conversation.
///
/// What Tab does, and the reason it is a separate command from the follow-up
/// below rather than a flag on one: appending to whatever came before, forever,
/// is exactly the behaviour this replaces, and a boolean argument is a thing
/// somebody can get wrong at a call site. Two names cannot be.
///
/// Returns the whole answer as well, so a caller that only wants the text does
/// not have to reassemble it from the events.
#[tauri::command]
pub(crate) async fn ai_ask(
    app: AppHandle,
    prefs: State<'_, PrefsState>,
    question: String,
) -> Result<String, String> {
    let chosen = who_answers(&prefs).await?;

    // Before the request, not after: the conversation is named by its first
    // question whether or not the answer ever arrives.
    app.state::<Chat>().begin(&question, crate::state::now_seconds());

    crate::ai::chat::ask(&app, &chosen, &question).await
}

/// Asks the next question of the conversation already open.
#[tauri::command]
pub(crate) async fn ai_follow_up(
    app: AppHandle,
    prefs: State<'_, PrefsState>,
    question: String,
) -> Result<String, String> {
    let chosen = who_answers(&prefs).await?;
    crate::ai::chat::ask(&app, &chosen, &question).await
}

/// Sets the open conversation aside so the next question begins its own.
///
/// Not `ai_clear`, which forgets everything. The one set aside is still
/// offered back from the root list until it goes stale.
#[tauri::command]
pub(crate) fn ai_new(chat: State<'_, Chat>) {
    chat.set_aside();
}

/// Reopens a conversation, and answers with everything said in it.
#[tauri::command]
pub(crate) fn ai_resume(chat: State<'_, Chat>, id: String) -> Result<Vec<Turn>, String> {
    if !chat.resume(&id, crate::state::now_seconds()) {
        return Err("That conversation is no longer here.".to_string());
    }

    Ok(chat.transcript())
}

/// The chosen provider, or why there is not one.
///
/// The same two checks in front of both ways of asking. Written once, because
/// the failure it guards against is a request going out shaped for a provider
/// that has no address.
async fn who_answers(prefs: &State<'_, PrefsState>) -> Result<Provider, String> {
    let settings = prefs.inner.lock().await.ai.clone();

    let chosen = chosen(&settings).ok_or_else(|| {
        "Nothing is set up to answer. Choose a provider in Settings.".to_string()
    })?;

    match what_is_missing(&chosen) {
        Some(missing) => Err(missing),
        None => Ok(chosen),
    }
}

/// One model somebody can choose.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Model {
    /// What goes into the request.
    pub id: String,
    /// What the settings window shows.
    pub label: String,
}

/// Which models a provider offers.
///
/// Asked rather than typed. A model id is a string, and one character wrong is
/// a request that fails with a message about a model nobody meant to ask for.
///
/// An empty list is not a failure: the settings window offers a text field
/// instead of a picker, which still works. A service that will not say what it
/// has should not stop somebody naming a model themselves.
#[tauri::command]
pub(crate) async fn ai_models(provider: Provider) -> Result<Vec<Model>, String> {
    if provider.wire == provider::Wire::ClaudeCode {
        // Its own aliases rather than an endpoint. Claude Code resolves
        // `sonnet` to whichever model that currently means.
        return Ok(crate::ai::claude_code::MODELS
            .iter()
            .map(|(id, label)| Model {
                id: (*id).to_string(),
                label: (*label).to_string(),
            })
            .collect());
    }

    if let Some(waiting_on) = nothing_to_ask_with(&provider) {
        return Err(waiting_on);
    }

    let client = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|err| format!("could not prepare the request: {err}"))?;

    let ids = crate::ai::openai::models(&client, &provider).await?;

    Ok(ids
        .into_iter()
        .map(|id| Model {
            label: id.clone(),
            id,
        })
        .collect())
}

/// What is still missing before a provider can be asked for its models.
///
/// A remote service with no key yet is not a failure to report. It is the
/// ordinary first second of setting one up: the address arrives from the table
/// and the key arrives when somebody pastes it. Asking anyway earns a 401 and
/// puts the provider's own refusal on screen, which reads as something being
/// broken rather than as one field being empty.
///
/// Decided in Rust because "local" is defined in Rust. A model on this machine
/// needs no key, and its list is there to be had from the first moment.
fn nothing_to_ask_with(provider: &Provider) -> Option<String> {
    if provider.base_url.trim().is_empty() {
        return Some("Give it an address and the models will list themselves.".to_string());
    }

    if provider.api_key.trim().is_empty() && !provider::is_on_this_network(&provider.base_url) {
        return Some("Paste a key and the models will list themselves.".to_string());
    }

    None
}

/// What each of these models is called, in order.
///
/// One call for a whole list rather than one per row. The alternative is the
/// window working the names out for itself, and then the rule for what a model
/// is called lives in two places: the launcher's chip would shorten one way
/// and the settings window another, and nothing would make them agree.
#[tauri::command]
pub(crate) fn ai_named(providers: Vec<Provider>) -> Vec<String> {
    providers
        .iter()
        .map(|one| provider::short_model(one.wire, &one.model))
        .collect()
}

/// The services Sill knows how to reach, for the settings window.
#[tauri::command]
pub(crate) fn ai_known() -> Vec<Offer> {
    provider::KNOWN
        .iter()
        .map(|known| Offer {
            provider: known.provider(),
            note: known.note.to_string(),
        })
        .collect()
}

/// A service on offer, with the line explaining what setting it up involves.
///
/// The note matters more here than anywhere: three of these have a
/// subscription with the same name that does not pay for the thing being set
/// up, and somebody about to paste a key should be told which.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Offer {
    #[serde(flatten)]
    pub provider: Provider,
    pub note: String,
}

/// The chosen provider, or the only one if only one is set up.
///
/// Falling back to the only one is not a guess: somebody who has configured
/// exactly one provider and never opened the chooser means that one.
fn chosen(settings: &crate::preferences::Ai) -> Option<Provider> {
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
fn what_is_missing(chosen: &Provider) -> Option<String> {
    if chosen.wire == provider::Wire::ClaudeCode {
        return crate::ai::claude_code::locate().is_none().then(|| {
            "Claude Code is not installed, or not somewhere Sill can find it."
                .to_string()
        });
    }

    if chosen.base_url.trim().is_empty() {
        return Some(format!("{} has no address.", chosen.name));
    }

    if let Err(refused) = provider::check(&chosen.base_url) {
        return Some(refused.message().to_string());
    }

    if chosen.model.trim().is_empty() {
        return Some(format!("No model is chosen for {}.", chosen.name));
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::preferences::Ai;

    fn provider(id: &str, base: &str, model: &str) -> Provider {
        Provider {
            id: id.into(),
            name: id.into(),
            base_url: base.into(),
            model: model.into(),
            ..Provider::default()
        }
    }

    mod before_it_is_worth_asking {
        use super::*;

        /// The moment after adding one from the table: an address, no key.
        /// Asking earns a 401, and the provider's own refusal on screen reads
        /// as something being broken.
        #[test]
        fn a_remote_one_with_no_key_says_what_it_is_waiting_for() {
            let waiting = nothing_to_ask_with(&provider("xai", "https://api.x.ai/v1", "grok-4"));
            assert!(waiting.unwrap_or_default().contains("Paste a key"));
        }

        /// A model on this machine needs no key, and its list is there from
        /// the first moment. Demanding one would put a text box in front of
        /// the one provider that can always answer.
        #[test]
        fn a_local_one_needs_no_key() {
            assert_eq!(
                nothing_to_ask_with(&provider("ollama", "http://localhost:11434/v1", "")),
                None,
            );
        }

        #[test]
        fn one_on_this_network_needs_no_key_either() {
            assert_eq!(
                nothing_to_ask_with(&provider("lan", "http://192.168.1.9:1234/v1", "")),
                None,
            );
        }

        #[test]
        fn no_address_says_so_rather_than_asking_nowhere() {
            let waiting = nothing_to_ask_with(&provider("blank", "", ""));
            assert!(waiting.unwrap_or_default().contains("address"));
        }

        #[test]
        fn a_remote_one_with_a_key_is_worth_asking() {
            let mut one = provider("xai", "https://api.x.ai/v1", "grok-4");
            one.api_key = "xai-something".into();
            assert_eq!(nothing_to_ask_with(&one), None);
        }
    }

    mod who_answers {
        use super::*;

        #[test]
        fn the_chosen_one_answers() {
            let settings = Ai {
                provider: "b".into(),
                providers: vec![
                    provider("a", "https://a.example/v1", "m"),
                    provider("b", "https://b.example/v1", "m"),
                ],
            };

            assert_eq!(chosen(&settings).map(|p| p.id), Some("b".to_string()));
        }

        /// Somebody who has set up exactly one and never opened the chooser
        /// means that one. This is not a guess.
        #[test]
        fn the_only_one_answers_when_nothing_is_chosen() {
            let settings = Ai {
                provider: String::new(),
                providers: vec![provider("a", "https://a.example/v1", "m")],
            };

            assert_eq!(chosen(&settings).map(|p| p.id), Some("a".to_string()));
        }

        /// Two set up and none chosen is a question for the person, not a coin
        /// toss: one of them may cost money and the other may not.
        #[test]
        fn two_set_up_and_none_chosen_is_nobody() {
            let settings = Ai {
                provider: String::new(),
                providers: vec![
                    provider("a", "https://a.example/v1", "m"),
                    provider("b", "https://b.example/v1", "m"),
                ],
            };

            assert!(chosen(&settings).is_none());
        }

        /// A choice pointing at something deleted falls back rather than
        /// answering with nothing.
        #[test]
        fn a_choice_that_no_longer_exists_falls_back_to_the_only_one() {
            let settings = Ai {
                provider: "deleted".into(),
                providers: vec![provider("a", "https://a.example/v1", "m")],
            };

            assert_eq!(chosen(&settings).map(|p| p.id), Some("a".to_string()));
        }

        #[test]
        fn nothing_set_up_is_nobody() {
            assert!(chosen(&Ai::default()).is_none());
        }
    }

    mod what_is_still_needed {
        use super::*;

        #[test]
        fn a_complete_one_needs_nothing() {
            assert_eq!(
                what_is_missing(&provider("a", "https://a.example/v1", "gpt-5.2")),
                None,
            );
        }

        /// Saying which thing is missing beats a request that fails with
        /// somebody else's error message.
        #[test]
        fn a_missing_model_says_so_rather_than_asking_anyway() {
            let missing = what_is_missing(&provider("a", "https://a.example/v1", ""));
            assert!(missing.unwrap_or_default().contains("No model"));
        }

        #[test]
        fn a_missing_address_says_so() {
            let missing = what_is_missing(&provider("a", "", "m"));
            assert!(missing.unwrap_or_default().contains("no address"));
        }

        /// The address rule is enforced here as well as at the request, so
        /// somebody is told before they type a question rather than after.
        #[test]
        fn an_address_a_key_may_not_be_sent_to_says_so() {
            let missing = what_is_missing(&provider("a", "http://example.com/v1", "m"));
            assert!(missing.unwrap_or_default().contains("https"));
        }
    }
}
