/*!
Talking to somebody else's MCP server, on a pipe, with a clock running.

Everything else in this module is Sill answering. This is Sill asking, and the
difference is the whole of why the file exists rather than being a few more
functions next door. [`super::link`] serves a client that Sill's own bridge
started; here the program on the far end is one **the person configured**, and
Sill knows nothing about it: not what it does, not how long it takes, not
whether it is still there.

## Nothing is held between calls

There is no connection, no pool and no handle anywhere. [`call`] starts the
program, says hello, asks one thing, reads the answer and ends the process
before it returns. A configured server that nobody is using is a few strings in
the preferences file and no more, which is what rule 23 asks for and what a
kept connection could not honestly claim: a stdio server is a process, and a
process kept warm is resident memory bought for a feature nobody invoked.

The cost of that choice is a process start on every call, which for a Node or
Python server is a few hundred milliseconds. It is paid by somebody who has
just chosen the row, never by the panel and never at rest. See
[`crate::actions::mcp`] for the half of that claim the panel depends on.

## Every path kills the child

The deadline is the point of this file. A server may be slow, may hang, may
answer with nonsense, or may have been uninstalled since it was configured, and
all four have to end in a sentence rather than a wait. So:

- [`STARTING`] bounds the handshake, which is where a program that starts and
  then says nothing gets caught.
- [`ANSWERING`] bounds the call itself.
- The child is `kill_on_drop`, so **every** way out of this function ends it,
  including the timeout, an early `?`, and a panic.
- It is also put in a [`crate::job::Job`], because an MCP server written in
  Node spawns more Node, and killing the one Sill started leaves the rest.

## What is deliberately not here

No notification methods, no resources, no prompts, no sampling and no roots. Sill
asks a server for its tools and calls one. A client that spoke the whole
protocol would be a great deal of surface answering questions nothing here
asks, and the parts of MCP that matter to a launcher are the two methods below.
*/

use std::process::Stdio;
use std::time::Duration;

use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

/// How long a server gets to start and answer `initialize`.
///
/// Generous, because it covers a cold `node` or `python` starting, which on a
/// machine that has just booted can genuinely take seconds. It is not generous
/// enough to be mistaken for no limit: a server that has not said hello in ten
/// seconds is one somebody needs to be told about.
pub const STARTING: Duration = Duration::from_secs(10);

/// How long one tool call gets before Sill stops waiting.
///
/// Longer than the handshake because a tool may legitimately be doing work: a
/// search, a network round trip, a build. Shorter than forever because the
/// person is looking at a launcher waiting for a row they pressed.
pub const ANSWERING: Duration = Duration::from_secs(60);

/// The revision Sill asks for.
///
/// The newest one [`super::protocol::REVISIONS`] lists, so the client and the
/// server halves of Sill agree about what MCP is by construction rather than
/// by two constants that have to be kept level.
fn revision() -> &'static str {
    super::protocol::REVISIONS[0]
}

/// What Sill calls itself to somebody else's server.
const ME: &str = "sill";

/// How a configured server is started.
///
/// Borrowed rather than owned because the caller holds the preferences and
/// this needs them only for the length of the call.
#[derive(Debug, Clone, Copy)]
pub struct Program<'a> {
    /// What the person named this server, for the sentence when it goes wrong.
    pub name: &'a str,
    pub command: &'a str,
    pub args: &'a [String],
}

/// One tool a server offers.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Tool {
    pub name: String,
    /// What it says it does, which is what somebody picks it by. Empty when
    /// the server offered none, rather than absent, so the panel has a field.
    pub description: String,
}

/// Everything one exchange with a server needs.
///
/// A struct rather than four parameters because [`talk`] is the only function
/// that opens a process and both public entry points go through it; naming the
/// pieces is what stops a caller passing the deadline where the method goes.
struct Exchange<'a> {
    program: Program<'a>,
    method: &'static str,
    params: Value,
    patience: Duration,
}

/// Asks a server what it can do.
///
/// For the settings panel, where somebody is choosing which tool becomes an
/// action and cannot be expected to have read the server's source. **This is
/// the only place a server is started by anything other than somebody running
/// one of its actions**, and it is started because a person pressed Check.
pub async fn tools(program: Program<'_>) -> Result<Vec<Tool>, String> {
    let answered = talk(Exchange {
        program,
        method: "tools/list",
        params: json!({}),
        patience: STARTING,
    })
    .await?;

    let listed = answered
        .get("tools")
        .and_then(Value::as_array)
        .ok_or_else(|| format!("{} answered without a list of tools", program.name))?;

    Ok(listed
        .iter()
        .filter_map(|tool| {
            let name = tool.get("name").and_then(Value::as_str)?;

            // A tool with no name cannot be called and cannot be chosen, so it
            // is dropped rather than drawn as a blank row.
            if name.is_empty() {
                return None;
            }

            Some(Tool {
                name: name.to_string(),
                description: tool
                    .get("description")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
            })
        })
        .collect())
}

/// Calls one tool and answers with what it said, as text.
///
/// Text rather than the structured content, for the reason
/// [`super::protocol::answered`] gives in the other direction: every server
/// fills in `content`, only some fill in `structuredContent`, and the thing
/// this becomes is an [`crate::action::Outcome`] message somebody reads.
///
/// A server that sets `isError` is an error here. It is the server saying the
/// call did not work, and an action that reports success over the top of that
/// would be lying to the person who pressed the row.
pub async fn call(program: Program<'_>, tool: &str, arguments: Value) -> Result<String, String> {
    let answered = talk(Exchange {
        program,
        method: "tools/call",
        params: json!({ "name": tool, "arguments": arguments }),
        patience: ANSWERING,
    })
    .await?;

    let said = said(&answered);

    if answered
        .get("isError")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return Err(if said.is_empty() {
            format!("{tool} failed and said nothing about why")
        } else {
            said
        });
    }

    Ok(said)
}

/// The text out of a `tools/call` result.
///
/// Every text block, joined, because a server is free to answer in several and
/// keeping only the first would silently drop most of a long answer. Blocks
/// that are not text (an image, an embedded resource) are named rather than
/// dropped, so an answer that was entirely a picture does not read as an
/// answer that was empty.
///
/// A free function so the shapes below can be proved without a process.
fn said(result: &Value) -> String {
    let Some(blocks) = result.get("content").and_then(Value::as_array) else {
        return String::new();
    };

    let mut out: Vec<String> = Vec::new();

    for block in blocks {
        match block.get("type").and_then(Value::as_str) {
            Some("text") => {
                if let Some(text) = block.get("text").and_then(Value::as_str) {
                    out.push(text.to_string());
                }
            }
            Some(other) => out.push(format!("[{other}]")),
            None => {}
        }
    }

    out.join("\n").trim().to_string()
}

/// Starts the program, does the handshake, asks one thing, and stops it.
///
/// The whole lifetime of a server is this function. Nothing it opens outlives
/// the return, which is what makes "a configured server that nobody is using
/// costs nothing" a fact about the code rather than a promise.
async fn talk(exchange: Exchange<'_>) -> Result<Value, String> {
    let Exchange {
        program,
        method,
        params,
        patience,
    } = exchange;

    let mut child = tokio::process::Command::new(program.command)
        .args(program.args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        // Kept separate and never read. A server that logs to stderr is
        // ordinary; a server whose stderr Sill piped and then ignored would
        // stop the moment that pipe filled, which is a hang with no cause
        // anybody could find. Inherited would put its logs in Sill's console.
        .stderr(Stdio::null())
        // Every way out of this function, including the timeout below and a
        // panic, ends the process. There is no path that leaks one.
        .kill_on_drop(true)
        .spawn()
        .map_err(|err| format!("could not start {}: {err}", program.name))?;

    /*
     * The tree, not the process.
     *
     * An MCP server started with `npx` is a Node process that starts another
     * one, and `kill_on_drop` ends only the child Sill holds. The job object
     * is the same primitive `bounded.rs` uses on npm and for the same reason:
     * closing the last handle to it is what the kernel takes as the signal to
     * terminate everything inside.
     *
     * Held for the length of this function and dropped with it.
     */
    let _job = crate::job::Job::new();
    #[cfg(windows)]
    if let (Some(job), Some(handle)) = (&_job, child.raw_handle()) {
        job.adopt_raw(handle);
    }

    let mut writing = child
        .stdin
        .take()
        .ok_or_else(|| format!("{} has no way to be spoken to", program.name))?;
    let reading = child
        .stdout
        .take()
        .ok_or_else(|| format!("{} has no way to answer", program.name))?;
    let mut lines = BufReader::new(reading).lines();

    // The handshake and the question are one bounded stretch. Splitting the
    // clock in two would let a server that spent nine seconds on `initialize`
    // have the whole of the call deadline afterwards, which is not what the
    // person waiting is measuring.
    let asked = tokio::time::timeout(patience, async {
        say(&mut writing, hello()).await?;

        // The server's answer to `initialize`, read and discarded. Nothing
        // here adapts to a capability, so the only thing this line is for is
        // arriving: a server that never sends it is one that never started
        // properly, and this is where that is found out.
        expect(&mut lines, 1).await?;

        // Required by the protocol, and a notification, so nothing comes back.
        // A server that waits for it before answering anything else is
        // ordinary, which is why skipping it is a hang rather than a warning.
        say(&mut writing, initialized()).await?;

        say(&mut writing, request(2, method, params)).await?;
        expect(&mut lines, 2).await
    })
    .await;

    // Before the answer is looked at. The child is going away either way, and
    // closing its input is how a server is asked to stop rather than killed
    // mid-sentence.
    let _ = writing.shutdown().await;

    match asked {
        Ok(answered) => answered.map_err(|why| format!("{}: {why}", program.name)),
        Err(_) => Err(gave_up(program.name, patience)),
    }
}

/// What is said when a server does not answer in time.
///
/// Names the server and the limit. "The action failed" about a program the
/// person configured a month ago is not something anybody can act on, and the
/// two likeliest causes, a server that is no longer installed and one waiting
/// on something of its own, are both things they can check.
fn gave_up(name: &str, patience: Duration) -> String {
    format!(
        "{name} did not answer within {} seconds, so Sill stopped waiting for it \
         and closed it. Check that it still starts on its own.",
        patience.as_secs()
    )
}

/// The handshake, as this client sends it.
fn hello() -> Value {
    request(
        1,
        "initialize",
        json!({
            "protocolVersion": revision(),
            // Nothing is claimed. Sill offers no roots and answers no sampling
            // request, and a client that advertised either would be asked for
            // something it has no code to give.
            "capabilities": {},
            "clientInfo": { "name": ME, "version": env!("CARGO_PKG_VERSION") },
        }),
    )
}

/// The notification every server is entitled to before it is asked anything.
fn initialized() -> Value {
    json!({ "jsonrpc": "2.0", "method": "notifications/initialized" })
}

fn request(id: u64, method: &str, params: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params })
}

/// Writes one message, on its own line.
async fn say(writing: &mut tokio::process::ChildStdin, message: Value) -> Result<(), String> {
    let mut written =
        serde_json::to_vec(&message).map_err(|err| format!("could not write a message: {err}"))?;
    written.push(b'\n');

    writing
        .write_all(&written)
        .await
        .map_err(|err| format!("could not be spoken to: {err}"))?;

    writing
        .flush()
        .await
        .map_err(|err| format!("could not be spoken to: {err}"))
}

/**
Reads until the answer to `id` arrives, and hands back its result.

**Skipping rather than taking the next line is the whole of this function.** A
server may write a log line, a `notifications/message`, a progress notification
or an answer to something else before the one being waited for, and a client
that treated the first line as the answer would fail on every server that is
chatty and work on every server that is not. That is the kind of bug that looks
like "it works with this server and not that one".

A line that is not JSON at all is skipped for the same reason: plenty of
programs print a banner on startup, and refusing outright would make a
perfectly good server unusable over one line of noise.

The end of the stream is an error, because a server that closed without
answering is one that fell over, and returning an empty answer would report
that as a tool that did nothing.
*/
async fn expect<R>(lines: &mut tokio::io::Lines<BufReader<R>>, id: u64) -> Result<Value, String>
where
    R: tokio::io::AsyncRead + Unpin,
{
    while let Some(line) = lines
        .next_line()
        .await
        .map_err(|err| format!("could not be read: {err}"))?
    {
        let Ok(message) = serde_json::from_str::<Value>(line.trim()) else {
            continue;
        };

        if message.get("id").and_then(Value::as_u64) != Some(id) {
            continue;
        }

        if let Some(failed) = message.get("error") {
            let why = failed
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("it refused and said nothing about why");
            return Err(why.to_string());
        }

        return message
            .get("result")
            .cloned()
            .ok_or_else(|| "answered with neither a result nor an error".to_string());
    }

    Err("stopped before it answered".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A reader over lines a fake server "wrote", for [`expect`].
    fn wrote(lines: &str) -> tokio::io::Lines<BufReader<std::io::Cursor<Vec<u8>>>> {
        BufReader::new(std::io::Cursor::new(lines.as_bytes().to_vec())).lines()
    }

    mod reading_an_answer {
        use super::*;

        #[tokio::test]
        async fn the_answer_to_the_question_that_was_asked() {
            let mut lines = wrote("{\"jsonrpc\":\"2.0\",\"id\":2,\"result\":{\"ok\":true}}\n");

            assert_eq!(expect(&mut lines, 2).await.unwrap(), json!({ "ok": true }));
        }

        /**
        Anything in front of it is skipped rather than mistaken for it.

        The failure this exists for looks like "it works with one server and not
        another". A server is free to write a log line, a progress notification
        or an answer to an earlier request before the one being waited for, and
        a client that read the next line and called it the answer would work
        perfectly against a quiet server and be broken against a chatty one.
        */
        #[tokio::test]
        async fn whatever_comes_first_is_stepped_over() {
            let mut lines = wrote(concat!(
                "starting up, listening on stdio\n",
                "{\"jsonrpc\":\"2.0\",\"method\":\"notifications/message\",\"params\":{}}\n",
                "{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"protocolVersion\":\"2025-06-18\"}}\n",
                "{\"jsonrpc\":\"2.0\",\"id\":2,\"result\":{\"content\":[]}}\n",
            ));

            assert_eq!(
                expect(&mut lines, 2).await.unwrap(),
                json!({ "content": [] })
            );
        }

        /// A refusal is the server's own words, not a generic failure.
        #[tokio::test]
        async fn an_error_carries_what_the_server_said() {
            let mut lines = wrote(
                "{\"jsonrpc\":\"2.0\",\"id\":2,\"error\":{\"code\":-32602,\"message\":\"no such tool\"}}\n",
            );

            assert_eq!(expect(&mut lines, 2).await.unwrap_err(), "no such tool");
        }

        /// A server that closed without answering fell over. Reporting that as
        /// an empty answer would show it as a tool that ran and did nothing.
        #[tokio::test]
        async fn a_stream_that_ends_first_is_a_failure_rather_than_an_empty_answer() {
            let mut lines = wrote("{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{}}\n");

            assert!(expect(&mut lines, 2)
                .await
                .unwrap_err()
                .contains("stopped before it answered"));
        }
    }

    mod what_a_call_answered {
        use super::*;

        #[test]
        fn text_blocks_are_joined_in_order() {
            let said = said(&json!({
                "content": [
                    { "type": "text", "text": "one" },
                    { "type": "text", "text": "two" },
                ]
            }));

            assert_eq!(said, "one\ntwo");
        }

        /// A picture is named rather than dropped. An answer that was entirely
        /// an image would otherwise read as an answer that was empty, and the
        /// person would be told the action did nothing.
        #[test]
        fn a_block_that_is_not_text_is_still_accounted_for() {
            let said = said(&json!({ "content": [{ "type": "image", "data": "..." }] }));

            assert_eq!(said, "[image]");
        }

        #[test]
        fn an_answer_with_no_content_is_empty_rather_than_a_panic() {
            assert_eq!(said(&json!({})), "");
            assert_eq!(said(&json!({ "content": [] })), "");
        }
    }

    mod the_handshake {
        use super::*;

        /// The revision asked for is one Sill's own server would agree to.
        ///
        /// Two constants would drift, and the drift is silent: a client asking
        /// for a revision the rest of Sill has never heard of still works,
        /// right up to the day somebody has to reason about which one is in
        /// use.
        #[test]
        fn it_asks_for_a_revision_sill_itself_speaks() {
            assert!(crate::ai::mcp::protocol::REVISIONS.contains(&revision()));
            assert_eq!(hello()["params"]["protocolVersion"], json!(revision()));
        }

        /// Nothing is claimed that nothing here can answer. A client
        /// advertising roots or sampling gets asked for them.
        #[test]
        fn it_offers_no_capability_it_cannot_honour() {
            assert_eq!(hello()["params"]["capabilities"], json!({}));
        }

        /// The notification that unblocks a server which waits for it. No id,
        /// because a notification with one is a request nothing will answer.
        #[test]
        fn the_notification_carries_no_id() {
            assert!(initialized().get("id").is_none());
            assert_eq!(initialized()["method"], json!("notifications/initialized"));
        }
    }

    /// The sentence somebody reads when a server hangs.
    #[test]
    fn giving_up_names_the_server_and_the_limit() {
        let said = gave_up("notes", Duration::from_secs(10));

        assert!(said.contains("notes"), "{said}");
        assert!(said.contains("10 seconds"), "{said}");
        assert!(said.contains("closed it"), "{said}");
    }

    /// A program that does not exist is a sentence rather than a wait.
    #[tokio::test]
    async fn a_command_that_is_not_there_fails_at_once() {
        let started = std::time::Instant::now();

        let refused = tools(Program {
            name: "ghost",
            command: "sill-no-such-program-exists",
            args: &[],
        })
        .await
        .expect_err("nothing should have started");

        assert!(refused.contains("ghost"), "{refused}");
        assert!(
            started.elapsed() < STARTING,
            "it waited {:?} for a program that cannot start",
            started.elapsed()
        );
    }

    /**
    A program that starts, holds its pipes and never answers is given up on.

    The case the whole file is shaped around, and the one an integration
    against a well-behaved server never reaches. `ping` runs for a minute,
    keeps stdin and stdout open the whole time, and writes lines that are not
    an answer to anything, which is what a server that has fallen over after
    starting looks like from here.

    It also drives the skipping in [`expect`] with real noise rather than a
    fixture: every line `ping` writes is stepped over, the deadline is what
    ends the wait, and neither the noise nor the silence is mistaken for a
    reply.

    Asked through `talk` directly rather than through [`tools`] so the deadline
    can be shortened. A test that proves a ten second limit by waiting ten
    seconds is a test nobody runs twice.
    */
    #[cfg(windows)]
    #[tokio::test]
    async fn a_server_that_never_answers_is_given_up_on_and_ended() {
        let started = std::time::Instant::now();

        let refused = talk(Exchange {
            program: Program {
                name: "silent",
                command: "cmd",
                args: &["/c".to_string(), "ping -n 60 127.0.0.1".to_string()],
            },
            method: "tools/list",
            params: json!({}),
            patience: Duration::from_millis(700),
        })
        .await
        .expect_err("a program that answers nothing must not be waited for");

        assert!(refused.contains("silent"), "{refused}");
        assert!(refused.contains("did not answer"), "{refused}");
        assert!(
            started.elapsed() < STARTING,
            "it waited {:?}, which is not a deadline",
            started.elapsed()
        );
    }
}
