//! The conversation, and asking the next thing in it.
//!
//! One conversation at a time, held here rather than in the window. The window
//! is closed most of the time and reloaded whenever the page does; a
//! conversation that lived there would be lost every time somebody pressed
//! Escape, which is the opposite of what a follow-up is for.
//!
//! ## What goes over the wire
//!
//! The whole conversation, every time, because that is how these services
//! work: they hold no state and the context is the request. The exception is
//! Claude Code, which holds the session itself and is handed only the new
//! question plus the id of the session to continue.
//!
//! ## Why the answer arrives as events
//!
//! A launcher that shows nothing for four seconds and then a paragraph feels
//! broken even when it is not. Each piece is emitted as it arrives and the
//! window appends it, so the first words are on screen while the rest is still
//! being written.

use std::sync::Mutex;

use serde::Serialize;
use tauri::{Emitter, Manager};

use super::openai::Message;
use super::provider::{Provider, Wire};

/// One exchange, as the window draws it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Turn {
    /// `user` or `assistant`.
    pub role: String,
    pub text: String,
}

/// The conversation so far.
#[derive(Default)]
struct Held {
    messages: Vec<Message>,
    /// Claude Code's own session, when that is who is answering.
    ///
    /// It keeps the conversation itself, so continuing means naming the
    /// session rather than sending everything again.
    session: Option<String>,
}

/// The one conversation, as managed state rather than a static.
#[derive(Default)]
pub struct Chat {
    held: Mutex<Held>,
}

impl Chat {
    pub fn new() -> Self {
        Self::default()
    }

    /// Everything said so far, for a window that has just opened.
    pub fn transcript(&self) -> Vec<Turn> {
        let Ok(held) = self.held.lock() else {
            return Vec::new();
        };

        held.messages
            .iter()
            .filter(|message| message.role != "system")
            .map(|message| Turn {
                role: message.role.clone(),
                text: message.content.clone(),
            })
            .collect()
    }

    /// Forgets it and starts again.
    pub fn clear(&self) {
        if let Ok(mut held) = self.held.lock() {
            held.messages.clear();
            held.session = None;
        }
    }

    fn remember(&self, message: Message) {
        if let Ok(mut held) = self.held.lock() {
            held.messages.push(message);
        }
    }

    fn context(&self) -> Vec<Message> {
        self.held
            .lock()
            .map(|held| held.messages.clone())
            .unwrap_or_default()
    }

    fn session(&self) -> Option<String> {
        self.held.lock().ok().and_then(|held| held.session.clone())
    }

    fn set_session(&self, session: String) {
        if let Ok(mut held) = self.held.lock() {
            held.session = Some(session);
        }
    }
}

/// What the window is told while an answer is being written.
const SAID: &str = "sill://ai-said";
const DONE: &str = "sill://ai-done";
const FAILED: &str = "sill://ai-failed";

/// Asks the next thing, and streams the answer to the window.
///
/// The question is remembered before the request rather than after, so a
/// conversation that fails half way still shows what was asked. The answer is
/// remembered when it is finished, because half an answer is not a turn.
pub async fn ask(
    app: &tauri::AppHandle,
    provider: &Provider,
    question: &str,
) -> Result<String, String> {
    let question = question.trim();
    if question.is_empty() {
        return Err("there is nothing to ask".to_string());
    }

    let chat = app.state::<Chat>();
    chat.remember(Message::user(question));

    let answer = match provider.wire {
        Wire::ClaudeCode => through_the_cli(app, provider, question).await,
        Wire::OpenAi => over_http(app, provider, &chat.context()).await,
        // Anthropic's own format is not written yet. Saying so is better than
        // sending it a request shaped for somebody else and reporting whatever
        // it says about that.
        Wire::Anthropic => Err(
            "Sill cannot speak to that one yet. Reach Anthropic through Claude \
             Code, or through a gateway that speaks the common format."
                .to_string(),
        ),
    };

    match answer {
        Ok(text) => {
            chat.remember(Message::assistant(&text));
            let _ = app.emit(DONE, ());
            Ok(text)
        }
        Err(why) => {
            let _ = app.emit(FAILED, &why);
            Err(why)
        }
    }
}

/// Over HTTP, to anything speaking the common shape.
async fn over_http(
    app: &tauri::AppHandle,
    provider: &Provider,
    messages: &[Message],
) -> Result<String, String> {
    let client = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|err| format!("could not prepare the request: {err}"))?;

    let mut whole = String::new();

    super::openai::ask(&client, provider, messages, |piece| {
        whole.push_str(&piece);
        let _ = app.emit(SAID, &piece);
    })
    .await?;

    Ok(whole)
}

/// Through the Claude Code binary, on the subscription.
///
/// Only the new question is sent: the CLI holds the conversation and is asked
/// to continue the session it gave us last time.
async fn through_the_cli(
    app: &tauri::AppHandle,
    provider: &Provider,
    question: &str,
) -> Result<String, String> {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

    let binary = super::claude_code::locate().ok_or_else(|| {
        "Claude Code is not installed, or not somewhere Sill can find it. \
         Install it, or choose a provider with a key instead."
            .to_string()
    })?;

    let chat = app.state::<Chat>();

    let working = super::claude_code::neutral_directory(
        &app.path()
            .app_data_dir()
            .map_err(|err| format!("no data directory: {err}"))?,
    );
    std::fs::create_dir_all(&working)
        .map_err(|err| format!("could not make a place to run from: {err}"))?;

    let mut command = tokio::process::Command::new(&binary);
    command
        .args(super::claude_code::arguments(
            chat.session().as_deref(),
            Some(&provider.model).filter(|m| !m.is_empty()).map(|m| m.as_str()),
        ))
        // The reason is in `claude_code.rs`: a session that is not bare runs
        // the hooks and servers of wherever it starts.
        .current_dir(&working)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null());

    for (name, value) in super::claude_code::environment(provider) {
        command.env(name, value);
    }

    #[cfg(windows)]
    {
        // No console window. Without this a black box flashes on screen every
        // time somebody asks a question.
        const NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(NO_WINDOW);
    }

    let mut child = command
        .spawn()
        .map_err(|err| format!("could not start Claude Code: {err}"))?;

    // The question goes in on stdin, so a long one cannot run into a
    // command-line length limit and nothing has to be quoted.
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(question.as_bytes()).await;
        // Closed, or it waits for more.
        drop(stdin);
    }

    let mut whole = String::new();
    let mut failure = None;

    if let Some(stdout) = child.stdout.take() {
        let mut lines = BufReader::new(stdout).lines();

        while let Ok(Some(line)) = lines.next_line().await {
            match super::claude_code::parse_event(&line) {
                super::claude_code::Event::Text(piece) => {
                    whole.push_str(&piece);
                    let _ = app.emit(SAID, &piece);
                }
                super::claude_code::Event::Session(id) => chat.set_session(id),
                super::claude_code::Event::Failed(why) => failure = Some(why),
                super::claude_code::Event::Done | super::claude_code::Event::Ignored => {}
            }
        }
    }

    let _ = child.wait().await;

    match failure {
        Some(why) => Err(why),
        None if whole.trim().is_empty() => {
            Err("Claude Code answered with nothing. It may need signing in: run \
                 `claude` once in a terminal."
                .to_string())
        }
        None => Ok(whole),
    }
}
