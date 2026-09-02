//! The two ends of one socket.
//!
//! Sill listens on the loopback interface; `sill.exe --mcp` connects to it and
//! copies bytes between that socket and its own stdin and stdout. The client
//! that started the bridge sees an ordinary MCP server on a pipe and never
//! learns there is a second process, and the running Sill answers every
//! message, because the running Sill is the only thing that can.
//!
//! ## The bridge is a pump and nothing else
//!
//! It parses no JSON, knows no methods and holds no state. That is deliberate:
//! a bridge that understood the protocol would be a second implementation of
//! it, in a process with no tests running against it, that has to be kept in
//! step with the first. Bytes in, bytes out, and one secret at the start.
//!
//! ## One message at a time, per connection
//!
//! The read loop answers a message before reading the next. An acting tool can
//! wait a minute and a half on somebody deciding, and that does hold up
//! anything else asked down the same socket, which is a real cost and the
//! right one: the alternative is running tool calls concurrently and then
//! having to decide what happens to a half-run action when the client hangs
//! up. Nothing is worth cancelling a file move part way through.
//!
//! If the client does give up while a card is on screen, the person's answer
//! still runs. They were asked and they said yes; the asker having walked off
//! is not a reason to disobey them.

use std::io::Write as _;
use std::net::Ipv4Addr;
use std::path::PathBuf;

use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};

use super::protocol::{self, Reply};

/// Where the bridge should connect, and what it must say when it does.
#[derive(Debug, Clone)]
pub struct Reachable {
    pub port: u16,
    pub token: String,
}

/// The listener, started the first time anything needs it.
///
/// Not at startup. Most sessions never ask a model anything, and a port bound
/// for a feature nobody used is a resource held for nothing. Once it is up it
/// costs nothing at all: a listener waiting on a connection is asleep, not
/// polling.
#[derive(Default)]
pub struct Link {
    started: tokio::sync::OnceCell<Reachable>,
}

impl Link {
    pub fn new() -> Self {
        Self::default()
    }

    /// Where it can be reached, starting it if this is the first time.
    ///
    /// A failure leaves the cell empty rather than remembering it, so a port
    /// that could not be bound this second is tried again next time rather
    /// than turning one bad moment into a dead feature until Sill restarts.
    pub async fn reachable(&self, app: &tauri::AppHandle) -> Result<Reachable, String> {
        self.started
            .get_or_try_init(|| async { start(app.clone()).await })
            .await
            .cloned()
    }
}

/// Binds a port and starts answering on it.
async fn start(app: tauri::AppHandle) -> Result<Reachable, String> {
    // Port zero, so the operating system picks a free one. A fixed port is a
    // port that is sometimes already taken, and by something that is not Sill.
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .await
        .map_err(|err| format!("could not open a port for MCP: {err}"))?;

    let port = listener
        .local_addr()
        .map_err(|err| format!("could not read the port: {err}"))?
        .port();

    let token = secret();
    let reachable = Reachable {
        port,
        token: token.clone(),
    };

    tauri::async_runtime::spawn(async move {
        serve(listener, token, move |name, arguments| {
            let app = app.clone();
            async move { crate::ai::tools::run(&app, &name, &arguments).await }
        })
        .await;
    });

    Ok(reachable)
}

/// Answers connections until the listener goes away.
///
/// The handler is passed in rather than reached for, so the whole transport
/// can be driven in a test by something that is not the tool catalogue. It is
/// `ai::tools::run` in the one place that matters.
pub async fn serve<F, Fut>(listener: TcpListener, token: String, run: F)
where
    F: Fn(String, Value) -> Fut + Clone + Send + 'static,
    Fut: std::future::Future<Output = Value> + Send,
{
    loop {
        let Ok((socket, _)) = listener.accept().await else {
            // The listener itself failed, which is not something a retry loop
            // can fix and is not worth spinning on.
            return;
        };

        let token = token.clone();
        let run = run.clone();

        tauri::async_runtime::spawn(async move {
            talk(socket, &token, run).await;
        });
    }
}

/// One connection, from the secret to the last message.
async fn talk<F, Fut>(socket: TcpStream, token: &str, run: F)
where
    F: Fn(String, Value) -> Fut,
    Fut: std::future::Future<Output = Value>,
{
    // Nagle off. Every message is small and is wanted immediately, and waiting
    // to see whether another one turns up is exactly the wrong trade for a
    // request and response that alternate.
    let _ = socket.set_nodelay(true);

    let (reading, mut writing) = socket.into_split();
    let mut lines = BufReader::new(reading).lines();

    // The secret first, before a single message is read. Anything else on this
    // machine can reach a loopback port; this is the whole of what stops it.
    match lines.next_line().await {
        Ok(Some(said)) if said.trim() == token => {}
        // Nothing is written back. A caller that does not know the secret
        // learns only that the connection closed, which is all it is owed.
        _ => return,
    }

    while let Ok(Some(line)) = lines.next_line().await {
        let reply = match protocol::dispatch(&line) {
            Reply::Nothing => continue,
            Reply::Now(value) => value,
            Reply::Call {
                id,
                name,
                arguments,
            } => protocol::answered(id, &run(name, arguments).await),
        };

        let Ok(mut written) = serde_json::to_vec(&reply) else {
            continue;
        };
        written.push(b'\n');

        if writing.write_all(&written).await.is_err() {
            // The client hung up. Nothing left to answer.
            return;
        }
    }
}

/// A secret for this run of Sill.
///
/// `RtlGenRandom` rather than the `windows` crate's cryptography feature, for
/// the reason `secrets.rs` gives: this crate's feature list has already pushed
/// rustc into an out of memory abort by accumulating, and one extern
/// declaration costs nothing.
fn secret() -> String {
    let mut bytes = [0u8; 32];

    #[cfg(windows)]
    {
        #[link(name = "advapi32")]
        extern "system" {
            #[link_name = "SystemFunction036"]
            fn rtl_gen_random(buffer: *mut u8, length: u32) -> u8;
        }

        // Zero means it refused, which has no documented cause and no useful
        // fallback. An all zero secret would be a guessable one, so the port
        // is better off unreachable than open with a known password.
        let ok = unsafe { rtl_gen_random(bytes.as_mut_ptr(), bytes.len() as u32) };
        assert!(ok != 0, "Windows would not produce random bytes");
    }

    #[cfg(not(windows))]
    {
        // Sill is a Windows program. This exists so the module compiles for a
        // test run elsewhere, and it is deliberately useless as a secret.
        unimplemented!("no secret source outside Windows");
    }

    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Whether this process was started as the bridge rather than as the launcher.
///
/// Read before Tauri is built, because a bridge that reaches the single
/// instance plugin is a bridge that toggles somebody's launcher window every
/// time a question is asked.
pub fn asked_for_bridge() -> bool {
    std::env::args().any(|argument| argument == super::FLAG)
}

/// Where this executable is, for a config that names it.
pub fn this_program() -> Result<PathBuf, String> {
    std::env::current_exe().map_err(|err| format!("Sill cannot find its own program: {err}"))
}

/// The bridge. Copies bytes between the client's pipes and the running Sill.
///
/// Returns what the process should exit with. A failure here is reported on
/// stderr and never on stdout: stdout is the protocol, and one stray line on
/// it is a parse error in the client rather than a message anybody reads.
pub fn bridge() -> i32 {
    use std::io::{copy, stderr, stdin, stdout};
    use std::net::{Shutdown, TcpStream};

    let Some(port) = std::env::var(super::PORT)
        .ok()
        .and_then(|p| p.parse::<u16>().ok())
    else {
        let _ = writeln!(stderr(), "sill --mcp: {} is not set", super::PORT);
        return 2;
    };

    let token = std::env::var(super::TOKEN).unwrap_or_default();
    if token.is_empty() {
        let _ = writeln!(stderr(), "sill --mcp: {} is not set", super::TOKEN);
        return 2;
    }

    let Ok(mut out) = TcpStream::connect((Ipv4Addr::LOCALHOST, port)) else {
        let _ = writeln!(
            stderr(),
            "sill --mcp: nothing is listening on {port}. Sill is not running.",
        );
        return 3;
    };

    let _ = out.set_nodelay(true);

    if writeln!(out, "{token}").is_err() {
        let _ = writeln!(stderr(), "sill --mcp: could not say who it was");
        return 3;
    }

    let Ok(mut back) = out.try_clone() else {
        let _ = writeln!(
            stderr(),
            "sill --mcp: could not use the connection both ways"
        );
        return 3;
    };

    // Everything the client says, on a thread of its own.
    std::thread::spawn(move || {
        let _ = copy(&mut stdin().lock(), &mut out);
        // Closing stdin is how a client asks a server to stop. Half closing
        // the socket passes that on rather than dropping the whole connection
        // while an answer may still be on its way back.
        let _ = out.shutdown(Shutdown::Write);
    });

    // Everything Sill says, on this one. When Sill closes the socket this
    // returns, the process ends, and the client sees its server exit, which is
    // exactly what it should see.
    let mut showing = stdout().lock();
    let _ = copy(&mut back, &mut showing);
    let _ = showing.flush();

    0
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// A listener answering with a handler that records what it was asked.
    async fn listening() -> (u16, String) {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let token = "a-secret".to_string();

        let said = token.clone();
        tokio::spawn(async move {
            serve(listener, said, |name, arguments| async move {
                json!({ "ran": name, "with": arguments })
            })
            .await;
        });

        (port, token)
    }

    /// Connects, says the given secret, and sends the lines.
    ///
    /// Answers with whatever came back before the connection closed, which is
    /// nothing at all when the secret was wrong.
    async fn ask(port: u16, token: &str, lines: &[&str]) -> Vec<Value> {
        let socket = TcpStream::connect((Ipv4Addr::LOCALHOST, port))
            .await
            .unwrap();
        let (reading, mut writing) = socket.into_split();

        writing
            .write_all(format!("{token}\n").as_bytes())
            .await
            .unwrap();
        for line in lines {
            writing
                .write_all(format!("{line}\n").as_bytes())
                .await
                .unwrap();
        }
        writing.shutdown().await.unwrap();

        let mut heard = Vec::new();
        let mut back = BufReader::new(reading).lines();

        while let Ok(Some(line)) = back.next_line().await {
            heard.push(serde_json::from_str(&line).expect("that was not JSON"));
        }

        heard
    }

    /// The whole of what stops anything else on the machine driving Sill.
    #[tokio::test]
    async fn the_wrong_secret_is_answered_with_nothing() {
        let (port, _) = listening().await;

        let heard = ask(
            port,
            "not-the-secret",
            &[r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#],
        )
        .await;

        assert!(
            heard.is_empty(),
            "it answered a caller that did not know the secret"
        );
    }

    /// The secret is a whole line and must match the whole line. A check that
    /// only looked at the start would open the port to any prefix of it.
    #[tokio::test]
    async fn a_secret_that_merely_starts_right_is_not_enough() {
        let (port, _) = listening().await;

        let heard = ask(
            port,
            "a-secre",
            &[r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#],
        )
        .await;

        assert!(heard.is_empty(), "a prefix of the secret was accepted");
    }

    #[tokio::test]
    async fn the_right_secret_gets_the_handshake_and_the_tools() {
        let (port, token) = listening().await;

        let heard = ask(
            port,
            &token,
            &[
                r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18"}}"#,
                r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
                r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#,
            ],
        )
        .await;

        // Two, not three. The notification in the middle gets no answer, and
        // an extra line here would be the client's ids no longer lining up.
        assert_eq!(heard.len(), 2, "{heard:?}");
        assert_eq!(heard[0]["id"], json!(1));
        assert_eq!(heard[1]["id"], json!(2));

        let offered = heard[1]["result"]["tools"].as_array().unwrap();
        assert_eq!(offered.len(), crate::ai::tools::CATALOGUE.len());
    }

    /// The point of the whole module: a call arrives, the tool runs, and the
    /// answer goes back under the id that asked for it.
    #[tokio::test]
    async fn a_call_reaches_the_handler_and_the_answer_comes_back() {
        let (port, token) = listening().await;

        let heard = ask(
            port,
            &token,
            &[
                r#"{"jsonrpc":"2.0","id":8,"method":"tools/call","params":{"name":"find_files","arguments":{"query":"notes"}}}"#,
            ],
        )
        .await;

        assert_eq!(heard.len(), 1, "{heard:?}");
        assert_eq!(heard[0]["id"], json!(8));

        let text = heard[0]["result"]["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("find_files"), "{text}");
        assert!(text.contains("notes"), "{text}");
    }

    /// Each message is its own line, and the newline is the only thing telling
    /// a client where one ends.
    ///
    /// The last of these carries a newline inside the answer, which is the way
    /// this actually breaks: the tools return text read off this machine, a
    /// file or a screen is full of newlines, and a transport that wrote its
    /// messages out with any formatting at all would split one answer into
    /// several fragments that parse as nothing.
    #[tokio::test]
    async fn every_answer_is_its_own_line() {
        let (port, token) = listening().await;

        let heard = ask(
            port,
            &token,
            &[
                r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#,
                r#"{"jsonrpc":"2.0","id":2,"method":"ping"}"#,
                r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"read_file","arguments":{"path":"one\ntwo\nthree"}}}"#,
            ],
        )
        .await;

        assert_eq!(heard.len(), 3, "{heard:?}");

        let text = heard[2]["result"]["content"][0]["text"].as_str().unwrap();
        assert!(
            text.contains("one\\ntwo"),
            "the newlines did not survive: {text}"
        );
    }

    #[test]
    fn a_secret_is_long_and_not_the_same_one_twice() {
        let one = secret();
        let two = secret();

        assert_eq!(one.len(), 64, "{one}");
        assert_ne!(one, two, "the same secret came back twice");
        assert!(one.chars().any(|c| c != '0'), "the secret is all zeroes");
    }
}
