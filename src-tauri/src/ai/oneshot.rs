//! One question, one answer, and no conversation to keep.
//!
//! `chat::ask` is the model as a participant: it pushes into the open
//! conversation, streams to the chat window and may reach for the tools. A
//! grammar fix, a translation or a transcript rewritten in another register is
//! none of those things. It is an instruction, one piece of text, and the
//! reply, and it must never read a file on the way. So this is a second door
//! onto the same providers: the same wire, the same key and the same stop flag
//! as the chat, and none of the chat's state.
//!
//! Nothing here exists until it is called, and nothing outlives the call.

use std::time::Duration;

use tauri::Manager;

use super::openai::Message;
use super::provider::{Provider, Wire};

/// The sentence the chat uses for the wire nobody has written yet.
///
/// Kept in step with `chat.rs` by a test that reads it, so the two doors
/// cannot describe the same shut door differently.
pub(crate) const NOT_SPOKEN_YET: &str = "Sill cannot speak to that one yet. Reach Anthropic through Claude \
     Code, or through a gateway that speaks the common format.";

/// Asks once: `system` is the instruction, `user` is the text it applies to.
///
/// The answer comes back whole and trimmed. An empty answer is a failure
/// rather than an empty string, because every caller is about to put the
/// answer somewhere a person is looking, and nothing there is a bug they
/// would report.
pub async fn complete(
    app: &tauri::AppHandle,
    provider: &Provider,
    system: &str,
    user: &str,
) -> Result<String, String> {
    let answer = match provider.wire {
        Wire::OpenAi => over_http(app, provider, system, user).await?,
        Wire::ClaudeCode => through_the_cli(app, provider, system, user).await?,
        Wire::Anthropic => return Err(NOT_SPOKEN_YET.to_string()),
    };

    let answer = answer.trim().to_string();
    if answer.is_empty() {
        return Err(format!("{} answered with nothing", provider.name));
    }

    Ok(answer)
}

/// The two messages a one-shot sends, in the order the wire wants them.
fn messages(system: &str, user: &str) -> Vec<Message> {
    vec![Message::system(system), Message::user(user)]
}

/// Over HTTP, to anything speaking the common shape.
///
/// No tools, which is the whole difference from the chat's loop: a request
/// with no tools cannot come back asking to read a file, so there is no loop
/// to run and no card to raise.
async fn over_http(
    app: &tauri::AppHandle,
    provider: &Provider,
    system: &str,
    user: &str,
) -> Result<String, String> {
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(15))
        .build()
        .map_err(|err| format!("could not prepare the request: {err}"))?;

    // Read and dropped on one line, as the chat does: a managed state guard
    // held across an await makes the future not `Send`.
    let since = app.state::<super::approval::Halt>().mark();
    let give_up = || app.state::<super::approval::Halt>().stopped(since);

    let said = super::openai::ask(
        &client,
        provider,
        &messages(system, user),
        None,
        &give_up,
        |_| {},
    )
    .await?;

    if said.stopped {
        return Err("stopped before it finished".to_string());
    }

    Ok(said.text)
}

/// What the Claude Code binary is started with for a one-shot.
///
/// The chat's arguments with no session to resume and no tools to offer,
/// plus the instruction, which the CLI takes on its own flag rather than as
/// part of the text. Separate from the spawn so the shape can be checked
/// without a binary.
fn cli_arguments(provider: &Provider, system: &str) -> Vec<String> {
    let model = Some(provider.model.as_str()).filter(|m| !m.is_empty());
    let mut args = super::claude_code::arguments(None, model, None);
    args.push("--append-system-prompt".to_string());
    args.push(system.to_string());
    args
}

/// Through the Claude Code binary on this machine.
///
/// The same spawn the chat performs, minus everything that makes a chat a
/// chat: no session id, no MCP config, no window told anything. Text events
/// are collected and returned once the turn is over.
async fn through_the_cli(
    app: &tauri::AppHandle,
    provider: &Provider,
    system: &str,
    user: &str,
) -> Result<String, String> {
    use super::claude_code::Event;
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

    let binary = super::claude_code::locate().ok_or_else(|| {
        "Claude Code is not installed, or not somewhere Sill can find it.".to_string()
    })?;

    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|err| format!("no data directory: {err}"))?;

    // The reason is in `claude_code.rs`: a session that is not bare runs the
    // hooks and servers of wherever it starts, so it starts nowhere.
    let working = super::claude_code::neutral_directory(&data_dir);
    std::fs::create_dir_all(&working)
        .map_err(|err| format!("could not make a place to run from: {err}"))?;

    let mut command = tokio::process::Command::new(&binary);
    command
        .args(cli_arguments(provider, system))
        .current_dir(&working)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null());

    for (name, value) in super::claude_code::environment(provider) {
        command.env(name, value);
    }

    #[cfg(windows)]
    {
        // No console window, or a black box flashes every time a key is
        // pressed over a paragraph.
        const NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(NO_WINDOW);
    }

    let mut child = command
        .spawn()
        .map_err(|err| format!("could not start Claude Code: {err}"))?;

    // The text goes in on stdin, so its length is nobody's problem and
    // nothing has to be quoted.
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(user.as_bytes()).await;
        drop(stdin);
    }

    let since = app.state::<super::approval::Halt>().mark();
    let give_up = || app.state::<super::approval::Halt>().stopped(since);

    let mut text = String::new();
    let mut failure = None;

    if let Some(stdout) = child.stdout.take() {
        let mut lines = BufReader::new(stdout).lines();

        while let Ok(Some(line)) = lines.next_line().await {
            if give_up() {
                // Killed rather than left to finish, or a stopped request
                // keeps spending on an answer nobody will read.
                let _ = child.kill().await;
                return Err("stopped before it finished".to_string());
            }

            match super::claude_code::parse_event(&line) {
                Event::Text(piece) => text.push_str(&piece),
                Event::Failed(why) => failure = Some(why),
                Event::Done => break,
                // Sessions, thinking, tool traffic: a one-shot has no use for
                // any of it, and the stream may grow new kinds.
                _ => {}
            }
        }
    }

    let _ = child.wait().await;

    match failure {
        Some(why) => Err(why),
        None => Ok(text),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cli_provider(model: &str) -> Provider {
        Provider {
            id: "claudeCode".into(),
            name: "Claude Code".into(),
            wire: Wire::ClaudeCode,
            model: model.into(),
            ..Provider::default()
        }
    }

    #[test]
    fn the_instruction_comes_first_and_the_text_second() {
        let sent = messages("Fix the grammar.", "their going home");

        assert_eq!(sent.len(), 2);
        assert_eq!(sent[0].role, "system");
        assert_eq!(sent[0].content, "Fix the grammar.");
        assert_eq!(sent[1].role, "user");
        assert_eq!(sent[1].content, "their going home");
    }

    /// The request over HTTP names no tools. Checked at the call rather than
    /// in the body, because the body is the OpenAI module's own business and
    /// this is the one line that decides whether a rewrite can read a file.
    #[test]
    fn the_request_over_http_names_no_tools() {
        // `include_str!` hands over whatever the checkout wrote. The pattern
        // below spans two lines, so a machine that checks out CRLF would fail a
        // test about tools for a reason that has nothing to do with tools.
        let source = include_str!("oneshot.rs").replace("\r\n", "\n");
        assert!(
            source.contains("&messages(system, user),\n        None,"),
            "the HTTP one-shot must pass None where the chat passes its tools"
        );
    }

    #[test]
    fn the_cli_is_given_the_instruction_on_its_own_flag() {
        let args = cli_arguments(&cli_provider("claude-sonnet-5"), "Translate into French.");

        let at = args
            .iter()
            .position(|arg| arg == "--append-system-prompt")
            .expect("the instruction flag");
        assert_eq!(args[at + 1], "Translate into French.");
        assert!(args.iter().any(|arg| arg == "claude-sonnet-5"));
    }

    #[test]
    fn the_cli_resumes_nothing_and_offers_no_tools() {
        let args = cli_arguments(&cli_provider(""), "Summarise.");

        assert!(!args.iter().any(|arg| arg == "--resume"));
        assert!(!args.iter().any(|arg| arg == "--mcp-config"));
    }

    /// The chat and the one-shot refuse the same wire with the same words.
    #[test]
    fn the_unwritten_wire_is_refused_in_the_chats_own_words() {
        let chat = include_str!("chat.rs");

        for piece in [
            "Sill cannot speak to that one yet",
            "speaks the common format",
        ] {
            assert!(NOT_SPOKEN_YET.contains(piece));
            assert!(chat.contains(piece), "chat.rs no longer says {piece:?}");
        }
    }
}
