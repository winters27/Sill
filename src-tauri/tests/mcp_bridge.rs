//! The bridge, as a real process.
//!
//! Everything else about MCP is checked inside one process: the protocol
//! against a string, the transport against a socket opened in the test. This
//! starts the actual `sill.exe --mcp` that a client would start, talks to it
//! down real pipes, and checks that what goes in one end comes out the other.
//!
//! It is the only thing that can catch the ways a separate process goes wrong
//! and a library does not: a flag read differently from how it is written, an
//! environment variable that never arrives, a pump that buffers until exit, a
//! stdout somebody wrote a diagnostic to.
//!
//! The listener here answers with a stand-in rather than the tool catalogue,
//! because the catalogue needs a running Sill and this needs to run anywhere.
//! What is being checked is the pipe, not the tools.

use std::io::{BufRead, BufReader, Write};
use std::net::Ipv4Addr;
use std::process::{Child, Command, Stdio};

use serde_json::{json, Value};

/// A listener with a stand-in handler, and the secret it expects.
fn listening() -> (u16, String, tokio::runtime::Runtime) {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("no runtime");

    let token = "bridge-test-secret".to_string();

    let listener = runtime.block_on(async {
        tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .await
            .expect("no port")
    });

    let port = listener.local_addr().expect("no address").port();

    let said = token.clone();
    runtime.spawn(async move {
        sill_lib::ai::mcp::link::serve(listener, said, |name, arguments| async move {
            json!({ "ran": name, "with": arguments })
        })
        .await;
    });

    // Held and returned, because dropping a runtime stops everything on it.
    (port, token, runtime)
}

/// Starts the real bridge, pointed at the given port.
fn bridge(port: u16, token: &str) -> Child {
    Command::new(env!("CARGO_BIN_EXE_sill"))
        .arg("--mcp")
        .env("SILL_MCP_PORT", port.to_string())
        .env("SILL_MCP_TOKEN", token)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the bridge would not start")
}

/// Writes the lines to its stdin and reads what comes back before it ends.
fn through(mut child: Child, lines: &[&str]) -> Vec<Value> {
    {
        let mut asking = child.stdin.take().expect("no stdin");
        for line in lines {
            writeln!(asking, "{line}").expect("could not write");
        }
        // Closing stdin is how a client says it is finished. Without it this
        // waits on a process that is waiting on us.
    }

    let out = BufReader::new(child.stdout.take().expect("no stdout"));

    let heard: Vec<Value> = out
        .lines()
        .map_while(Result::ok)
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(&line).unwrap_or_else(|_| panic!("not JSON: {line}")))
        .collect();

    let _ = child.wait();

    heard
}

/// The whole path, in the shape a client actually uses it.
#[test]
fn a_client_talking_to_the_real_bridge_gets_the_handshake_and_a_tool_answer() {
    let (port, token, _runtime) = listening();

    let heard = through(
        bridge(port, &token),
        &[
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"a test","version":"1"}}}"#,
            r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
            r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#,
            r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"list_windows","arguments":{}}}"#,
        ],
    );

    // Three, not four. The notification gets no answer, and an extra line here
    // would be every id after it lining up against the wrong request.
    assert_eq!(heard.len(), 3, "{heard:?}");

    assert_eq!(heard[0]["result"]["serverInfo"]["name"], json!("sill"));
    assert_eq!(heard[0]["result"]["protocolVersion"], json!("2025-06-18"));

    let offered = heard[1]["result"]["tools"].as_array().expect("no tools");
    assert!(
        offered.iter().any(|tool| tool["name"] == json!("list_windows")),
        "{offered:?}",
    );

    assert_eq!(heard[2]["id"], json!(3));
    let text = heard[2]["result"]["content"][0]["text"]
        .as_str()
        .expect("no text came back");
    assert!(text.contains("list_windows"), "{text}");
}

/// Stdout is the protocol. One diagnostic written to it is a parse error in
/// the client rather than a line anybody reads, and it would be a line the
/// client can never resynchronise after.
#[test]
fn nothing_but_the_protocol_is_written_to_stdout() {
    let (port, token, _runtime) = listening();

    let heard = through(
        bridge(port, &token),
        &[r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#],
    );

    assert_eq!(heard.len(), 1, "{heard:?}");
    assert_eq!(heard[0]["result"], json!({}));
}

/// A bridge that cannot reach Sill has to end rather than sit there.
///
/// A client waiting on a server that will never answer looks exactly like a
/// slow one, and there is nothing to wait for: Sill is not running.
#[test]
fn a_bridge_with_nothing_to_connect_to_gives_up_and_says_so() {
    // Bound and dropped, so it is a port nothing is listening on rather than a
    // number guessed at, which could be somebody else's server.
    let taken = std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("no port");
    let port = taken.local_addr().expect("no address").port();
    drop(taken);

    let finished = bridge(port, "any-secret")
        .wait_with_output()
        .expect("it never ended");

    assert!(!finished.status.success(), "it claimed to have worked");
    assert!(finished.stdout.is_empty(), "it wrote to stdout: {:?}", finished.stdout);

    let said = String::from_utf8_lossy(&finished.stderr);
    assert!(said.contains("Sill is not running"), "{said}");
}

/// Started with no environment, it is a bridge with no idea where to go.
#[test]
fn a_bridge_told_nothing_refuses_rather_than_guessing() {
    let finished = Command::new(env!("CARGO_BIN_EXE_sill"))
        .arg("--mcp")
        .env_remove("SILL_MCP_PORT")
        .env_remove("SILL_MCP_TOKEN")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("it would not start");

    assert!(!finished.status.success());
    assert!(String::from_utf8_lossy(&finished.stderr).contains("SILL_MCP_PORT"));
}
