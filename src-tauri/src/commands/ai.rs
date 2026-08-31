//! What the window can ask of the AI side.

use tauri::{AppHandle, State};

use crate::ai::chat::{Chat, Turn};
use crate::ai::provider::{self, Provider};
use crate::state::PrefsState;

/// Who is set up to answer, and who is chosen.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Ready {
    /// Whether anything is set up at all.
    pub ready: bool,
    /// What the chosen one is called, for a line saying who answered.
    pub name: String,
    /// Why not, when not. Empty when it is ready.
    pub why_not: String,
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
            name: String::new(),
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
            name: chosen.name.clone(),
            why_not: missing,
        });
    }

    Ok(Ready {
        ready: true,
        name: chosen.name.clone(),
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

/// Asks, and streams the answer to the window as events.
///
/// Returns the whole answer as well, so a caller that only wants the text does
/// not have to reassemble it from the events.
#[tauri::command]
pub(crate) async fn ai_ask(
    app: AppHandle,
    prefs: State<'_, PrefsState>,
    question: String,
) -> Result<String, String> {
    let settings = prefs.inner.lock().await.ai.clone();

    let chosen = chosen(&settings).ok_or_else(|| {
        "Nothing is set up to answer. Choose a provider in Settings.".to_string()
    })?;

    if let Some(missing) = what_is_missing(&chosen) {
        return Err(missing);
    }

    crate::ai::chat::ask(&app, &chosen, &question).await
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
