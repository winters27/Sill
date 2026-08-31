//! Asking through Claude Code, which is already installed and already signed in.
//!
//! ## Why this exists at all
//!
//! Anthropic banned third-party tools from using a Claude subscription in
//! April 2026, reinstated it on 13 May, and on 15 June paused the change that
//! would have moved programmatic use onto separate credits. What stands today
//! is that the Agent SDK, `claude -p`, and third-party tools continue to draw
//! on the ordinary subscription pools.
//!
//! So the sanctioned way to reach a subscription is **the official CLI the
//! person already has**, rather than reimplementing its sign-in. That
//! distinction is the whole thing: minting OAuth tokens ourselves is what was
//! banned; running the tool that already holds them is what Zed and JetBrains
//! do.
//!
//! ## Two facts from the documentation that decide the design
//!
//! **Not `--bare`.** Bare mode "never reads OAuth credentials or the system
//! keychain", which is precisely the thing wanted here. It is the right flag
//! for CI and the wrong one for this.
//!
//! **A working directory with nothing in it.** The cost of not using `--bare`
//! is that a `-p` session "runs the hooks in a project's
//! `.claude/settings.json` and connects the servers in its `.mcp.json`, even
//! in a folder you've never trusted", with no trust dialog and no per-server
//! prompt. Sill asks questions on somebody's behalf; it must not do that from
//! a folder that can answer with code execution. So it runs from a directory
//! of Sill's own, containing nothing.
//!
//! ## Every provider through one path
//!
//! Claude Code reads `ANTHROPIC_BASE_URL` and `ANTHROPIC_AUTH_TOKEN`, so it
//! can be pointed at anything speaking the same shape. Set neither and it uses
//! the subscription.
//!
//! Tools that do this write those into `~/.claude/settings.json`, which is the
//! user's own file and their own Claude Code setup. **Sill sets them in the
//! environment of the process it spawns and nowhere else.** Nothing on disk
//! changes, there is nothing to restore if Sill is killed half way, and two
//! questions asked a second apart can go to different providers.

use std::path::{Path, PathBuf};

use super::provider::Provider;

/// What one line of the stream said.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// More of the answer.
    Text(String),
    /// Which conversation this is, so a follow-up can continue it.
    Session(String),
    /// The turn is over.
    Done,
    /// It went wrong, and this is what it said.
    Failed(String),
    /// Something this does not need to know about.
    Ignored,
}

/// Reads one line of `--output-format stream-json`.
///
/// Anything unrecognised is ignored rather than treated as a failure. The
/// stream carries tool calls, retries, plugin loads and subagent traffic, and
/// a chat window needs none of it; a new event type in a future release must
/// not break the answer arriving.
pub fn parse_event(line: &str) -> Event {
    let line = line.trim();
    if line.is_empty() {
        return Event::Ignored;
    }

    let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
        // Not JSON at all. The stream is newline-delimited JSON, so this is a
        // stray line rather than something to report.
        return Event::Ignored;
    };

    let kind = value.get("type").and_then(|t| t.as_str()).unwrap_or("");

    match kind {
        // A token, which is the only thing a chat window is really waiting for.
        "stream_event" => {
            let delta = value.pointer("/event/delta");
            let is_text = delta
                .and_then(|d| d.get("type"))
                .and_then(|t| t.as_str())
                == Some("text_delta");

            match delta.and_then(|d| d.get("text")).and_then(|t| t.as_str()) {
                Some(text) if is_text => Event::Text(text.to_string()),
                _ => Event::Ignored,
            }
        }

        // The last line of the stream. It carries the session id, which is
        // what makes a follow-up possible.
        "result" => {
            let failed = value
                .get("is_error")
                .and_then(|e| e.as_bool())
                .unwrap_or(false);

            if failed {
                let said = value
                    .get("result")
                    .and_then(|r| r.as_str())
                    .unwrap_or("that request failed");
                return Event::Failed(said.to_string());
            }

            Event::Done
        }

        "system" => {
            // The first event, which names the session.
            if value.get("subtype").and_then(|s| s.as_str()) == Some("init") {
                if let Some(id) = value.get("session_id").and_then(|s| s.as_str()) {
                    return Event::Session(id.to_string());
                }
            }

            Event::Ignored
        }

        _ => Event::Ignored,
    }
}

/// The arguments for one question.
///
/// The prompt is not among them: it goes in on stdin, so a long question
/// cannot run into a command-line length limit and nothing has to be quoted
/// for a shell that is not involved.
pub fn arguments(session: Option<&str>, model: Option<&str>) -> Vec<String> {
    let mut args: Vec<String> = vec![
        // Non-interactive. Deliberately without `--bare`, which would skip the
        // OAuth credentials that are the entire reason for coming this way.
        "-p".into(),
        // Tokens as they are produced, rather than a wall of text at the end.
        "--output-format".into(),
        "stream-json".into(),
        "--verbose".into(),
        "--include-partial-messages".into(),
        /*
         * Nothing runs on this machine.
         *
         * A launcher's chat window is a place to ask a question, not a place
         * to hand something a shell. `dontAsk` denies anything not explicitly
         * allowed, and nothing is allowed. There is no interactive prompt in a
         * spawned process anyway, so the alternative is not "the user
         * decides", it is a request that hangs.
         */
        "--permission-mode".into(),
        "dontAsk".into(),
    ];

    if let Some(session) = session.filter(|s| !s.is_empty()) {
        args.push("--resume".into());
        args.push(session.to_string());
    }

    if let Some(model) = model.filter(|m| !m.trim().is_empty()) {
        args.push("--model".into());
        args.push(model.trim().to_string());
    }

    args
}

/// The models this can be asked for.
///
/// Aliases rather than ids. Claude Code resolves `sonnet` to whichever model
/// that currently means, and pinning an id here would freeze somebody on an
/// old one every time a new release lands. The empty first entry means
/// "whatever Claude Code is already set to", which is the right default: a
/// choice made in Claude Code itself should not be overridden by a launcher.
pub const MODELS: &[(&str, &str)] = &[
    ("", "Whatever Claude Code is set to"),
    ("fable", "Fable"),
    ("opus", "Opus"),
    ("sonnet", "Sonnet"),
    ("haiku", "Haiku"),
];

/// The environment overrides for one question, if any.
///
/// Empty means the subscription: Claude Code signed in as itself, reaching
/// Anthropic, billed the way the person's plan is billed.
///
/// A base URL means somewhere else, and then a token is needed too. The token
/// goes in `ANTHROPIC_AUTH_TOKEN` rather than `ANTHROPIC_API_KEY` because the
/// former becomes an `Authorization: Bearer` header, which is what a
/// compatible gateway expects, and because the latter is documented to
/// override the subscription even when it is empty of meaning.
pub fn environment(provider: &Provider) -> Vec<(String, String)> {
    let base = provider.base_url.trim();

    if base.is_empty() {
        return Vec::new();
    }

    let mut env = vec![("ANTHROPIC_BASE_URL".to_string(), base.to_string())];

    let key = provider.api_key.trim();
    if !key.is_empty() {
        env.push(("ANTHROPIC_AUTH_TOKEN".to_string(), key.to_string()));
    }

    env
}

/// Where the binary is, if it is anywhere.
///
/// `PATH` first, then the places the two documented installers put it. Not an
/// exhaustive search of the disk: a launcher that goes looking through
/// somebody's filesystem for an executable to run is doing something it should
/// not, and being told it is not installed is a better outcome than finding
/// something that happens to share the name.
pub fn locate() -> Option<PathBuf> {
    if let Some(found) = on_path() {
        return Some(found);
    }

    for candidate in likely_places() {
        if candidate.is_file() {
            return Some(candidate);
        }
    }

    None
}

fn on_path() -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    let names: &[&str] = if cfg!(windows) {
        &["claude.exe", "claude.cmd", "claude.bat"]
    } else {
        &["claude"]
    };

    for directory in std::env::split_paths(&path) {
        for name in names {
            let candidate = directory.join(name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }

    None
}

fn likely_places() -> Vec<PathBuf> {
    let mut out = Vec::new();

    if let Some(home) = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")) {
        let home = PathBuf::from(home);

        // Where the installer puts it.
        out.push(home.join(".local").join("bin").join("claude.exe"));
        out.push(home.join(".local").join("bin").join("claude"));

        // Where it ends up after moving off a global npm install, which is
        // still how a lot of existing setups look.
        out.push(home.join(".claude").join("local").join("claude.exe"));
        out.push(home.join(".claude").join("local").join("claude"));
    }

    if let Some(appdata) = std::env::var_os("APPDATA") {
        // Where a global npm install lands on Windows.
        out.push(PathBuf::from(appdata).join("npm").join("claude.cmd"));
    }

    out
}

/// A directory with nothing in it, to run from.
///
/// The reason is in the module note: a `-p` session without `--bare` runs the
/// hooks and MCP servers of whatever folder it starts in, with no trust
/// dialog. Sill's own empty directory has neither, so a question asked from
/// the launcher cannot be answered by somebody's repository running code.
pub fn neutral_directory(data_dir: &Path) -> PathBuf {
    data_dir.join("ask")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn provider(base: &str, key: &str) -> Provider {
        Provider {
            id: "claude".into(),
            name: "Claude".into(),
            base_url: base.into(),
            api_key: key.into(),
            ..Provider::default()
        }
    }

    mod where_it_is_looked_for {
        use super::*;

        /// Every place it is actually installed.
        ///
        /// Not on this machine's PATH is the ordinary case: the launcher runs
        /// as a desktop process, and a shell profile that put it on PATH is
        /// not read by one. So the fallback list is the part that has to be
        /// right, and it is the part nothing else exercises.
        #[test]
        fn the_fallback_list_covers_both_ways_it_is_installed() {
            let places = likely_places();
            let text: Vec<String> = places
                .iter()
                .map(|path| path.to_string_lossy().replace("\\", "/"))
                .collect();

            for expected in [".local/bin/claude", ".claude/local/claude", "npm/claude.cmd"] {
                assert!(
                    text.iter().any(|path| path.contains(expected)),
                    "nothing looks in {expected}: {text:?}",
                );
            }
        }

        /// Absolute, because the process is started somewhere with nothing in
        /// it and a relative path would resolve against that.
        #[test]
        fn every_place_is_an_absolute_path() {
            for place in likely_places() {
                assert!(place.is_absolute(), "{} is relative", place.display());
            }
        }
    }

    mod what_is_asked {
        use super::*;

        /// Bare mode "never reads OAuth credentials or the system keychain",
        /// which is the one thing this path exists for.
        #[test]
        fn it_is_never_asked_in_bare_mode() {
            let args = arguments(None, None);
            assert!(
                !args.iter().any(|a| a == "--bare"),
                "bare mode would skip the subscription: {args:?}",
            );
        }

        #[test]
        fn it_streams_rather_than_waiting_for_the_whole_answer() {
            let args = arguments(None, None);

            for wanted in ["-p", "stream-json", "--verbose", "--include-partial-messages"] {
                assert!(args.iter().any(|a| a == wanted), "{wanted} is missing");
            }
        }

        /// A chat window is a place to ask a question, not a place to hand
        /// something a shell.
        #[test]
        fn nothing_is_allowed_to_run_on_this_machine() {
            let args = arguments(None, None);

            let at = args.iter().position(|a| a == "--permission-mode");
            assert_eq!(at.map(|at| args[at + 1].as_str()), Some("dontAsk"));
            assert!(
                !args.iter().any(|a| a == "--allowedTools"),
                "something was allowed: {args:?}",
            );
        }

        #[test]
        fn a_follow_up_continues_the_same_conversation() {
            let args = arguments(Some("abc-123"), None);

            let at = args.iter().position(|a| a == "--resume");
            assert_eq!(at.map(|at| args[at + 1].as_str()), Some("abc-123"));
        }

        /// An empty session is no session, not a session called nothing.
        #[test]
        fn nothing_is_resumed_when_there_is_nothing_to_resume() {
            for session in [None, Some(""), Some("   ")] {
                let args = arguments(session.filter(|s| !s.trim().is_empty()), None);
                assert!(!args.iter().any(|a| a == "--resume"), "{session:?}");
            }
        }

        #[test]
        fn a_model_is_named_only_when_one_was_chosen() {
            assert!(!arguments(None, None).iter().any(|a| a == "--model"));
            assert!(!arguments(None, Some("  ")).iter().any(|a| a == "--model"));

            let args = arguments(None, Some("opus"));
            let at = args.iter().position(|a| a == "--model");
            assert_eq!(at.map(|at| args[at + 1].as_str()), Some("opus"));
        }
    }

    mod which_provider_answers {
        use super::*;

        /// Nothing set means the subscription, which is the point of coming
        /// this way at all.
        #[test]
        fn no_overrides_means_the_subscription() {
            assert!(environment(&provider("", "")).is_empty());
            assert!(environment(&provider("   ", "sk-whatever")).is_empty());
        }

        #[test]
        fn a_base_url_sends_it_somewhere_else() {
            let env = environment(&provider("https://gateway.example.com", "sk-abc"));

            assert!(env.contains(&(
                "ANTHROPIC_BASE_URL".into(),
                "https://gateway.example.com".into(),
            )));
            assert!(env.contains(&("ANTHROPIC_AUTH_TOKEN".into(), "sk-abc".into())));
        }

        /// `ANTHROPIC_API_KEY` is documented to override the subscription when
        /// set. Using it for a gateway token would mean a stray value in that
        /// variable silently changing who answers.
        #[test]
        fn the_key_never_goes_into_the_variable_that_overrides_the_subscription() {
            let env = environment(&provider("https://gateway.example.com", "sk-abc"));

            assert!(
                !env.iter().any(|(name, _)| name == "ANTHROPIC_API_KEY"),
                "{env:?}",
            );
        }

        /// A local model needs no token, and sending an empty one would be a
        /// header saying nothing.
        #[test]
        fn somewhere_that_needs_no_token_is_sent_none() {
            let env = environment(&provider("http://localhost:11434/v1", ""));

            assert_eq!(env.len(), 1);
            assert_eq!(env[0].0, "ANTHROPIC_BASE_URL");
        }
    }

    mod reading_the_stream {
        use super::*;

        #[test]
        fn a_token_comes_out_as_text() {
            let line = r#"{"type":"stream_event","event":{"delta":{"type":"text_delta","text":"Hello"}}}"#;
            assert_eq!(parse_event(line), Event::Text("Hello".into()));
        }

        /// Thinking and other deltas are not the answer.
        #[test]
        fn a_delta_that_is_not_text_is_not_text() {
            let line = r#"{"type":"stream_event","event":{"delta":{"type":"thinking_delta","thinking":"hmm"}}}"#;
            assert_eq!(parse_event(line), Event::Ignored);
        }

        #[test]
        fn the_session_is_read_from_the_first_event() {
            let line = r#"{"type":"system","subtype":"init","session_id":"abc-123"}"#;
            assert_eq!(parse_event(line), Event::Session("abc-123".into()));
        }

        #[test]
        fn the_last_line_ends_the_turn() {
            let line = r#"{"type":"result","is_error":false,"result":"Hello"}"#;
            assert_eq!(parse_event(line), Event::Done);
        }

        /// A failure inside the run is printed as the result rather than to
        /// stderr, so this is where a missing sign-in shows up.
        #[test]
        fn a_failure_says_what_went_wrong() {
            let line = r#"{"type":"result","is_error":true,"result":"not logged in"}"#;
            assert_eq!(parse_event(line), Event::Failed("not logged in".into()));
        }

        /// The stream carries tool calls, retries, plugin loads and subagent
        /// traffic. A chat window needs none of it, and a new event type in a
        /// future release must not stop the answer arriving.
        #[test]
        fn everything_else_is_ignored_rather_than_treated_as_a_failure() {
            for line in [
                r#"{"type":"assistant","message":{"content":[]}}"#,
                r#"{"type":"system","subtype":"api_retry","attempt":1}"#,
                r#"{"type":"user","parent_tool_use_id":"x"}"#,
                r#"{"type":"something_from_a_later_version"}"#,
                "",
                "   ",
                "not json at all",
                "{",
            ] {
                assert_eq!(parse_event(line), Event::Ignored, "{line}");
            }
        }
    }
}
