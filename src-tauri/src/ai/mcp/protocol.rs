//! The wire, and nothing else.
//!
//! MCP is JSON-RPC 2.0, one message per line, and this file turns a line into
//! either a finished answer or a tool to run. It touches no state, opens no
//! socket and knows nothing about Sill, which is the only reason the awkward
//! parts of the protocol can be proved in a unit test rather than by starting
//! a client and watching.
//!
//! ## The three things that are easy to get wrong
//!
//! **A notification gets no reply, ever.** A message with no `id` is a
//! notification, and answering one is a protocol violation rather than a
//! harmless extra line. `notifications/initialized` arrives on every single
//! connection, so getting this wrong is not a rare path.
//!
//! **The version is negotiated, not asserted.** A client names the revision it
//! speaks. If it is one this knows, it is echoed back unchanged; if it is not,
//! the answer names the newest one this knows and the client decides whether
//! to continue. Answering with our own version regardless is how a client that
//! speaks an older revision ends up talking to a server that thinks it agreed.
//!
//! **A method nobody advertised is an error, not silence.** `resources/list`
//! and `prompts/list` are asked for by clients that probe rather than read the
//! capabilities. `-32601` is the correct answer and it is the quiet one: a
//! silent drop leaves the client waiting for a response that never comes.

use serde_json::{json, Value};

/// The revisions this speaks.
///
/// Newest first, because the first entry is also the answer to a client
/// speaking something unknown. Every one of these carries tools in the same
/// shape, which is why all three are simply accepted rather than adapted to.
pub const REVISIONS: &[&str] = &["2025-06-18", "2025-03-26", "2024-11-05"];

/// What Sill calls itself when a client asks.
///
/// It is also the middle of every tool name the client sees, so an allow rule
/// written against `mcp__sill__` is written against this.
pub const SERVER: &str = "sill";

/// What one line asked for.
pub enum Reply {
    /// A finished response, ready to be written back.
    Now(Value),
    /// A tool to run, and the id the answer has to carry.
    Call {
        id: Value,
        name: String,
        arguments: Value,
    },
    /// Nothing goes back. A notification, a blank line, or a response to a
    /// request this never sent.
    Nothing,
}

/// Reads one line and says what happens next.
pub fn dispatch(line: &str) -> Reply {
    let line = line.trim();

    if line.is_empty() {
        return Reply::Nothing;
    }

    let Ok(message) = serde_json::from_str::<Value>(line) else {
        return Reply::Now(failed(Value::Null, -32700, "that was not JSON"));
    };

    // A batch. Removed from the protocol in the 2025-06-18 revision, and there
    // is no single id to answer one with, so it is refused whole.
    if message.is_array() {
        return Reply::Now(failed(
            Value::Null,
            -32600,
            "a batch is not part of this protocol",
        ));
    }

    let Some(method) = message.get("method").and_then(Value::as_str) else {
        // No method means this is a response to a request. Sill's server sends
        // none, so this is somebody else's traffic rather than a fault.
        return Reply::Nothing;
    };

    // The whole reason a notification is told apart from a request. No id
    // means no reply, and a reply would be the violation.
    let Some(id) = message.get("id").filter(|id| !id.is_null()).cloned() else {
        return Reply::Nothing;
    };

    let params = message.get("params").cloned().unwrap_or_else(|| json!({}));

    match method {
        "initialize" => Reply::Now(answer(id, initialized(&params))),
        "ping" => Reply::Now(answer(id, json!({}))),
        "tools/list" => Reply::Now(answer(id, json!({ "tools": crate::ai::tools::as_mcp() }))),
        "tools/call" => {
            let name = params
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();

            if name.is_empty() {
                return Reply::Now(failed(id, -32602, "no tool was named"));
            }

            Reply::Call {
                id,
                // Several of these take nothing, and clients differ on whether
                // that arrives as an empty object, a null, or not at all. A
                // tool reading its arguments must not be handed a null.
                arguments: params
                    .get("arguments")
                    .cloned()
                    .filter(Value::is_object)
                    .unwrap_or_else(|| json!({})),
                name,
            }
        }
        other => Reply::Now(failed(
            id,
            -32601,
            &format!("Sill's server has no method called {other}"),
        )),
    }
}

/// The handshake answer, carrying whichever revision was agreed.
fn initialized(params: &Value) -> Value {
    let asked = params
        .get("protocolVersion")
        .and_then(Value::as_str)
        .unwrap_or_default();

    let agreed = if REVISIONS.contains(&asked) {
        asked
    } else {
        REVISIONS[0]
    };

    json!({
        "protocolVersion": agreed,
        // Tools and nothing else. Resources and prompts are not offered
        // because there are none, and advertising an empty one earns a list
        // request per connection that answers with nothing.
        "capabilities": {
            // The list is a compile time constant, so it cannot change while a
            // client is connected and nothing will ever notify that it has.
            "tools": { "listChanged": false }
        },
        "serverInfo": {
            "name": SERVER,
            "version": env!("CARGO_PKG_VERSION"),
        }
    })
}

/// What a tool answered, in the shape `tools/call` returns.
///
/// The value goes back as text rather than as structured content, because
/// every client renders text and only some read the structured field. It is
/// pretty printed for the same reason Sill's own tool answers are: a model
/// reads it, and one long line of JSON is harder to follow than an indented
/// one.
///
/// `isError` is set from the answer rather than from whether anything went
/// wrong here. Sill's tools return a failure as an ordinary answer on purpose,
/// so a missing folder is a fact the turn carries rather than an exception;
/// the flag says which kind of fact it is without changing that.
pub fn answered(id: Value, value: &Value) -> Value {
    let text = serde_json::to_string_pretty(value)
        .unwrap_or_else(|_| String::from("{\"error\":\"that answer could not be written out\"}"));

    answer(
        id,
        json!({
            "content": [{ "type": "text", "text": text }],
            "isError": value.get("error").is_some(),
        }),
    )
}

fn answer(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn failed(id: Value, code: i32, why: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": code, "message": why }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now(line: &str) -> Value {
        match dispatch(line) {
            Reply::Now(value) => value,
            Reply::Call { .. } => panic!("that was a tool call, not an answer"),
            Reply::Nothing => panic!("that answered with nothing"),
        }
    }

    mod what_gets_a_reply {
        use super::*;

        /// The one that arrives on every connection.
        ///
        /// A notification has no id, so there is nothing to answer it with,
        /// and a reply carrying a null id is a protocol violation rather than
        /// a stray line. Every client sends this immediately after the
        /// handshake, so a server that answers it is wrong on every session
        /// rather than occasionally.
        #[test]
        fn a_notification_is_never_answered() {
            assert!(matches!(
                dispatch(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#),
                Reply::Nothing
            ));
        }

        /// Some clients write `"id": null` rather than leaving it out.
        #[test]
        fn a_null_id_is_a_notification_too() {
            assert!(matches!(
                dispatch(r#"{"jsonrpc":"2.0","id":null,"method":"notifications/cancelled"}"#),
                Reply::Nothing
            ));
        }

        #[test]
        fn a_blank_line_is_not_a_message() {
            assert!(matches!(dispatch("   "), Reply::Nothing));
        }

        /// Sill's server sends no requests, so anything shaped like a response
        /// to one belongs to somebody else.
        #[test]
        fn a_response_is_not_answered() {
            assert!(matches!(
                dispatch(r#"{"jsonrpc":"2.0","id":4,"result":{}}"#),
                Reply::Nothing
            ));
        }

        /// A client waiting on a reply that never comes hangs. Saying the
        /// method is unknown is both correct and the only thing that ends it.
        #[test]
        fn an_unknown_method_is_refused_rather_than_dropped() {
            let refused = now(r#"{"jsonrpc":"2.0","id":7,"method":"resources/list"}"#);

            assert_eq!(refused["id"], json!(7));
            assert_eq!(refused["error"]["code"], json!(-32601));
        }

        #[test]
        fn a_line_that_is_not_json_is_answered_with_a_parse_error() {
            let refused = now("not json at all");

            assert_eq!(refused["error"]["code"], json!(-32700));
            assert_eq!(refused["id"], Value::Null);
        }

        #[test]
        fn a_batch_is_refused_whole() {
            let refused = now(r#"[{"jsonrpc":"2.0","id":1,"method":"ping"}]"#);

            assert_eq!(refused["error"]["code"], json!(-32600));
        }

        /// An id may be a string, and answering with a number instead is a
        /// reply the client cannot match to anything it sent.
        #[test]
        fn the_id_comes_back_exactly_as_it_arrived() {
            let answered = now(r#"{"jsonrpc":"2.0","id":"abc","method":"ping"}"#);

            assert_eq!(answered["id"], json!("abc"));
        }
    }

    mod the_handshake {
        use super::*;

        /// Echoed, not asserted. A client speaking an older revision that is
        /// told a newer one has been agreed will use shapes this never sent.
        #[test]
        fn a_revision_this_knows_comes_back_unchanged() {
            for revision in REVISIONS {
                let line = format!(
                    "{{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\
                     \"params\":{{\"protocolVersion\":\"{revision}\"}}}}"
                );

                assert_eq!(
                    now(&line)["result"]["protocolVersion"],
                    json!(revision),
                    "{revision} was not echoed",
                );
            }
        }

        #[test]
        fn a_revision_this_does_not_know_is_answered_with_the_newest_one() {
            let line = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"1999-01-01"}}"#;

            assert_eq!(now(line)["result"]["protocolVersion"], json!(REVISIONS[0]));
        }

        /// The name here becomes the middle of every tool name the client
        /// sees, which is what the allow rules are written against.
        #[test]
        fn it_says_who_it_is() {
            let hello = now(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#);

            assert_eq!(hello["result"]["serverInfo"]["name"], json!(SERVER));
            assert!(hello["result"]["capabilities"]["tools"].is_object());
        }

        /// Nothing here serves resources or prompts, and a client told
        /// otherwise will ask for a list that cannot be answered.
        #[test]
        fn it_claims_only_what_it_has() {
            let hello = now(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#);
            let claimed = &hello["result"]["capabilities"];

            assert!(claimed.get("resources").is_none(), "claims resources");
            assert!(claimed.get("prompts").is_none(), "claims prompts");
        }
    }

    mod calling_one {
        use super::*;

        #[test]
        fn a_call_carries_the_name_the_arguments_and_the_id() {
            let line = r#"{"jsonrpc":"2.0","id":9,"method":"tools/call","params":{"name":"read_file","arguments":{"path":"C:/x.txt"}}}"#;

            match dispatch(line) {
                Reply::Call {
                    id,
                    name,
                    arguments,
                } => {
                    assert_eq!(id, json!(9));
                    assert_eq!(name, "read_file");
                    assert_eq!(arguments["path"], json!("C:/x.txt"));
                }
                _ => panic!("that was not read as a call"),
            }
        }

        /// Several tools take nothing, and clients differ on whether that
        /// arrives as an empty object, a null, or not at all. All three mean
        /// the same thing.
        #[test]
        fn missing_arguments_arrive_as_an_empty_object() {
            for line in [
                r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"list_windows"}}"#,
                r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"list_windows","arguments":null}}"#,
            ] {
                match dispatch(line) {
                    Reply::Call { arguments, .. } => {
                        assert!(arguments.is_object(), "{line} gave {arguments}")
                    }
                    _ => panic!("{line} was not read as a call"),
                }
            }
        }

        #[test]
        fn a_call_naming_no_tool_is_refused_rather_than_run() {
            let refused = now(r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{}}"#);

            assert_eq!(refused["error"]["code"], json!(-32602));
        }

        /// Sill answers a failure rather than raising one, and the flag is the
        /// only thing telling a client which kind of answer it is holding.
        #[test]
        fn an_answer_carrying_an_error_is_flagged_as_one() {
            let sad = answered(json!(1), &json!({ "error": "no such folder" }));
            let fine = answered(json!(1), &json!({ "found": 0, "results": [] }));

            assert_eq!(sad["result"]["isError"], json!(true));
            assert_eq!(fine["result"]["isError"], json!(false));
        }

        /// A refusal is not a failure. The model is meant to read it, say what
        /// it did not do, and carry on.
        #[test]
        fn somebody_saying_no_is_not_an_error() {
            let refused = answered(
                json!(1),
                &json!({ "done": false, "refused": true, "note": "they said no" }),
            );

            assert_eq!(refused["result"]["isError"], json!(false));
        }

        #[test]
        fn the_answer_is_carried_as_text_a_client_can_show() {
            let said = answered(json!(1), &json!({ "found": 2 }));
            let text = said["result"]["content"][0]["text"].as_str().unwrap();

            assert_eq!(said["result"]["content"][0]["type"], json!("text"));
            assert!(text.contains("\"found\""), "{text}");
        }
    }
}
