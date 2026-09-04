//! Sill calling somebody else's MCP server, as two real processes.
//!
//! `mcp_bridge.rs` is the other direction: a real client talking to the real
//! `sill.exe --mcp`. This is Sill as the client, starting a real program, doing
//! the real handshake down real pipes and reading a real answer back.
//!
//! It is the only thing that can catch what a unit test over a string cannot:
//! a message written without its newline, a flush that never happened, a
//! handshake the server was waiting on, a pipe that buffers until exit. The
//! unit tests in `ai/mcp/client.rs` prove the shapes; this proves the plumbing.
//!
//! The server is written out by the test rather than committed beside it,
//! because what it has to be is small and exact: a fixture in another file is
//! one somebody edits without seeing what depends on it. It is a genuine MCP
//! server all the same. It speaks JSON-RPC on stdio, one message per line,
//! answers `initialize`, waits for `notifications/initialized` before it will
//! answer anything else, and implements `tools/list` and `tools/call`.
//!
//! **Node is required.** So is it for the extension host, for `gate:views` and
//! for `host/test`, so a machine that can build Sill has it.

use std::path::PathBuf;

use sill_lib::ai::mcp::client::{self, Program};

/// A real MCP server, written where it can be started.
///
/// The awkward parts of the protocol are deliberately all here, because a
/// server that has none of them proves nothing:
///
/// - it prints a line of noise on startup, the way plenty of programs do, so
///   the client has to step over something that is not JSON;
/// - it sends a notification of its own before the answer, so the client has to
///   step over a message that is JSON and is not a reply;
/// - it will not answer anything until `notifications/initialized` arrives, so
///   a client that skips the notification hangs rather than failing loudly.
fn server_at(dir: &std::path::Path) -> PathBuf {
    let path = dir.join("server.mjs");

    std::fs::write(
        &path,
        r#"
import { createInterface } from "node:readline";
import { appendFileSync } from "node:fs";

let ready = false;

/*
 * Two argv slots, both for the test that proves nothing is left running.
 *
 * A byte a tick into a file is what makes "is it still alive" answerable from
 * the outside, and the delay before an answer is what keeps it alive long
 * enough for that to mean something: a server that lives for eighty
 * milliseconds cannot tell a kill apart from an exit.
 */
const heartbeat = process.argv[2];
const delay = Number(process.argv[3] ?? 0);

if (heartbeat) {
  appendFileSync(heartbeat, "x");
  setInterval(() => appendFileSync(heartbeat, "x"), 50);
}

// A banner on stderr would be invisible to the client, so it goes where it
// costs something: stdout, in front of the handshake answer.
process.stdout.write("mcp fixture listening on stdio\n");

const say = (message) => process.stdout.write(JSON.stringify(message) + "\n");

createInterface({ input: process.stdin }).on("line", (line) => {
  let message;
  try {
    message = JSON.parse(line);
  } catch {
    return;
  }

  if (message.method === "notifications/initialized") {
    ready = true;
    return;
  }

  if (message.id === undefined || message.id === null) return;

  if (message.method === "initialize") {
    say({
      jsonrpc: "2.0",
      id: message.id,
      result: {
        protocolVersion: message.params?.protocolVersion ?? "2025-06-18",
        capabilities: { tools: { listChanged: false } },
        serverInfo: { name: "fixture", version: "1.0.0" },
      },
    });
    return;
  }

  // Nothing else is answered until the client has finished the handshake.
  if (!ready) return;

  // A notification of the server's own, in front of every answer, so the
  // client has to look at ids rather than at whatever arrived next.
  say({ jsonrpc: "2.0", method: "notifications/message", params: { level: "info" } });

  if (message.method === "tools/list") {
    say({
      jsonrpc: "2.0",
      id: message.id,
      result: {
        tools: [
          { name: "shout", description: "Upper-cases the path it is given" },
          { name: "sulk", description: "Always fails" },
          { name: "", description: "Has no name and cannot be called" },
        ],
      },
    });
    return;
  }

  if (message.method === "tools/call") {
    const { name, arguments: args } = message.params ?? {};

    if (name === "sulk") {
      say({
        jsonrpc: "2.0",
        id: message.id,
        result: { content: [{ type: "text", text: "it would rather not" }], isError: true },
      });
      return;
    }

    if (name === "shout") {
      const answer = () =>
        say({
          jsonrpc: "2.0",
          id: message.id,
          result: {
            content: [
              { type: "text", text: String(args?.path ?? "").toUpperCase() },
              { type: "text", text: "done" },
            ],
          },
        });

      if (delay > 0) setTimeout(answer, delay);
      else answer();
      return;
    }

    say({
      jsonrpc: "2.0",
      id: message.id,
      error: { code: -32602, message: `no tool called ${name}` },
    });
  }
});
"#,
    )
    .expect("the fixture server is written");

    path
}

/// How the client is told to start it.
fn args(path: &std::path::Path) -> Vec<String> {
    vec![path.to_string_lossy().to_string()]
}

/// The fixture written into a scratch directory, ready to be started.
fn on(dir: &tempfile::TempDir) -> Vec<String> {
    args(&server_at(dir.path()))
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("a runtime")
}

/// What a server offers, read off a real one.
///
/// The unnamed third tool is dropped rather than drawn, because a row somebody
/// cannot press is worse than no row.
#[test]
fn a_real_server_is_asked_what_it_can_do() {
    let runtime = runtime();
    let dir = tempfile::tempdir().expect("a temp directory");
    let args = on(&dir);

    let offered = runtime
        .block_on(client::tools(Program {
            name: "fixture",
            command: "node",
            args: &args,
        }))
        .expect("the fixture answers");

    let names: Vec<&str> = offered.iter().map(|tool| tool.name.as_str()).collect();

    assert_eq!(names, ["shout", "sulk"], "{offered:?}");
    assert!(
        offered[0].description.contains("Upper-cases"),
        "the description is what somebody picks a tool by: {offered:?}"
    );
}

/// The whole point: a tool is called with the thing being acted on, and what
/// it said comes back.
#[test]
fn a_real_tool_is_called_and_answers() {
    let runtime = runtime();
    let dir = tempfile::tempdir().expect("a temp directory");
    let args = on(&dir);

    let said = runtime
        .block_on(client::call(
            Program {
                name: "fixture",
                command: "node",
                args: &args,
            },
            "shout",
            serde_json::json!({ "path": "C:/notes/todo.md" }),
        ))
        .expect("the tool ran");

    // Both blocks, in order. A client keeping only the first would silently
    // drop most of a long answer.
    assert_eq!(said, "C:/NOTES/TODO.MD\ndone");
}

/// A server saying the call failed is a failure here.
///
/// Reporting it as success would put the server's complaint in the launcher
/// under a sentence saying the action worked.
#[test]
fn a_tool_that_says_it_failed_is_a_failure() {
    let runtime = runtime();
    let dir = tempfile::tempdir().expect("a temp directory");
    let args = on(&dir);

    let refused = runtime
        .block_on(client::call(
            Program {
                name: "fixture",
                command: "node",
                args: &args,
            },
            "sulk",
            serde_json::json!({}),
        ))
        .expect_err("isError means it did not work");

    assert!(refused.contains("would rather not"), "{refused}");
}

/// A tool the server has never heard of is refused in the server's own words.
#[test]
fn a_tool_the_server_does_not_have_is_refused_by_name() {
    let runtime = runtime();
    let dir = tempfile::tempdir().expect("a temp directory");
    let args = on(&dir);

    let refused = runtime
        .block_on(client::call(
            Program {
                name: "fixture",
                command: "node",
                args: &args,
            },
            "whistle",
            serde_json::json!({}),
        ))
        .expect_err("there is no such tool");

    assert!(
        refused.contains("fixture"),
        "it does not say who: {refused}"
    );
    assert!(refused.contains("whistle"), "{refused}");
}

/**
Nothing is left running afterwards.

The claim the whole design rests on: a configured server costs nothing at rest,
because the process only exists for the length of one call. A client that
leaked one per invocation would look identical from the outside until somebody
had pressed the row forty times.

Measured by what the child is still doing rather than by counting processes,
the way `bounded.rs` learned to: the fixture writes a byte every fifty
milliseconds for as long as it lives, and a file that stops growing is a
process that stopped. Counting `node.exe` on the machine is a test that fails
whenever another one happens to be running.

**The delay is the positive control.** The fixture is told to take a second
before it answers, so the heartbeat has written twenty times or so by the time
the call returns; without it, a heartbeat that never worked at all and a
process that was killed correctly look exactly the same.
*/
#[test]
fn the_server_does_not_outlive_the_call() {
    let runtime = runtime();
    let dir = tempfile::tempdir().expect("a temp directory");
    let heartbeat = dir.path().join("alive.txt");

    let args = vec![
        server_at(dir.path()).to_string_lossy().to_string(),
        heartbeat.to_string_lossy().to_string(),
        "1000".to_string(),
    ];

    let said = runtime
        .block_on(client::call(
            Program {
                name: "fixture",
                command: "node",
                args: &args,
            },
            "shout",
            serde_json::json!({ "path": "x" }),
        ))
        .expect("the tool ran");

    assert_eq!(
        said,
        "X
done"
    );

    // The kill is asynchronous at the kernel level, so this is not the
    // measurement, it is waiting for the measurement to be fair.
    std::thread::sleep(std::time::Duration::from_secs(2));
    let settled = std::fs::metadata(&heartbeat)
        .map(|it| it.len())
        .unwrap_or(0);

    assert!(
        settled > 5,
        "the heartbeat wrote {settled} bytes while the server was alive for a          second, so this test cannot tell a kill from a fixture that never ran"
    );

    std::thread::sleep(std::time::Duration::from_secs(2));
    let later = std::fs::metadata(&heartbeat)
        .map(|it| it.len())
        .unwrap_or(0);

    assert_eq!(
        later, settled,
        "the server is still running two seconds after the call it was started for"
    );
}
