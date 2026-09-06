//! The same tools, reached by anything that speaks MCP.
//!
//! ## Why this exists
//!
//! Sill's tools reach a model over HTTP by being described in the request, and
//! `claude -p` has no request to put them in: it is a program, not an
//! endpoint, and it has no function calling interface. So the Claude Code
//! provider had nine reading tools and two acting ones that it could not use,
//! which is the one path that costs nothing to run and the one path that could
//! not look at the machine it was running on.
//!
//! MCP is the interface it does have. Exposing the tools that way fixes the
//! Claude Code provider and, for the same work, makes them reachable from any
//! other client: an editor, another agent, the `claude` command in a terminal.
//!
//! ## One list, two transports
//!
//! Nothing here describes a tool. `ai::tools::CATALOGUE` carries the name, the
//! description and the schema, `ai::tools::run` dispatches, and both the HTTP
//! shape and the MCP shape are derived from it. A second list would be a
//! second list to keep up to date, and the failure would be silent: a tool
//! added for the chat window that no MCP client can see, or worse, one
//! described differently in each place.
//!
//! ## Two processes, and why
//!
//! MCP over stdio means the client starts a program and talks to it down a
//! pipe. The tools need the running Sill and nothing else will do: the index
//! took a scan to build, the clipboard history is a database one process has
//! open, the window list is about this moment, and the approval card has to
//! appear in front of the person answering it. A fresh process has none of
//! that.
//!
//! So `sill.exe --mcp` is a bridge and not a server. It is started by the
//! client, connects back to the running Sill over the loopback interface, and
//! copies bytes in both directions. Every decision about the protocol is made
//! in the one place, by the process that can answer.
//!
//! ## What stops anything else connecting
//!
//! A loopback port is reachable by anything else running on the machine, so
//! the first line down the socket has to be a secret Sill made this run. It is
//! handed to the bridge in its environment by the client that starts it, and
//! it is minted fresh every time Sill starts, so a config file left behind by
//! a previous run opens nothing.
//!
//! The honest boundary is the same one `secrets.rs` describes: this does not
//! defend against a process already running as this user, which could read the
//! config file. It defends against everything that cannot, which on a machine
//! with more than one account is most things.
//!
//! ## The other direction
//!
//! Everything above is Sill answering. [`client`] is Sill asking: a server the
//! person configured, started down a pipe, asked one thing and ended. The two
//! halves share [`protocol::REVISIONS`] so there is one answer to what
//! revision of MCP Sill speaks, and share nothing else, because serving and
//! calling have almost no code in common.
//!
//! ## The approval card is not optional here
//!
//! Nothing in this module runs an action. `tools::run` does, through the same
//! registry, the same `Capability` and the same `Pending` the chat window
//! waits on. An MCP client asking for something that writes a file or launches
//! a program stops exactly where the chat window stops, and gets the same
//! refusal when nobody says yes.

pub mod client;
pub mod link;
pub mod protocol;

use std::path::{Path, PathBuf};

use serde_json::{json, Value};

/// How the bridge is told where to connect.
///
/// In the environment rather than in the arguments. A command line is readable
/// from any process listing on the machine; an environment block is not, which
/// makes it the better of two imperfect places for a secret that has to reach
/// a child process somehow.
pub const PORT: &str = "SILL_MCP_PORT";
pub const TOKEN: &str = "SILL_MCP_TOKEN";

/// The argument that makes this process a bridge rather than the launcher.
pub const FLAG: &str = "--mcp";

/// What every tool is called once a client has namespaced it.
///
/// Claude Code names an MCP tool `mcp__<server>__<tool>`, and that is the name
/// a permission rule has to match. Derived from the catalogue rather than
/// written out, so a tool added later is allowed by the same act that adds it.
pub fn allowed() -> Vec<String> {
    crate::ai::tools::CATALOGUE
        .iter()
        .map(|tool| format!("mcp__{}__{}", protocol::SERVER, tool.name))
        .collect()
}

/// A tool's own name, out of the namespaced one a client calls it by.
///
/// The window reads a step by the name in the catalogue, and a client reaches
/// the same tool as `mcp__sill__<name>`. Anything not namespaced this way is
/// somebody else's tool and keeps its name.
pub fn short_name(namespaced: &str) -> &str {
    let prefix = format!("mcp__{}__", protocol::SERVER);
    namespaced.strip_prefix(prefix.as_str()).unwrap_or(namespaced)
}

/// The document `--mcp-config` reads.
pub fn config(bridge: &Path, port: u16, token: &str) -> Value {
    json!({
        "mcpServers": {
            protocol::SERVER: {
                "type": "stdio",
                "command": bridge.to_string_lossy(),
                "args": [FLAG],
                "env": {
                    PORT: port.to_string(),
                    TOKEN: token,
                }
            }
        }
    })
}

/**
The config file, which removes itself when nobody needs it any more.

**It holds the secret that authorises the silent tools.** Anything reaching the
bridge with that token can read a file, the clipboard, the screen and the
selection, with no card raised and nothing to notice afterwards. The honest
boundary is the one `secrets.rs` already describes: this does not defend
against a process running as this user. What it does defend against is the file
still sitting in the sync folder tomorrow, which is what happened before: it
was written on the first question and never removed, so the token stayed
readable for the rest of the session and every backup took a copy.

A guard rather than a call at the end, so the error paths are covered too. A
question that fails, is cancelled, or ends in a panic still takes the file with
it, which is exactly when nobody would have remembered to tidy up.
*/
pub struct Config(PathBuf);

impl Config {
    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Config {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// Writes it where the CLI can be pointed at it, and says where that was.
///
/// Not in the directory the CLI is run from. That one is empty on purpose, so
/// that a session which reads the hooks and servers of wherever it starts
/// finds neither, and a file named for a server would be the one thing capable
/// of undoing that.
///
/// Rewritten on every question rather than kept, because the port and the
/// secret both change when Sill restarts and a stale file is a config naming a
/// server that cannot be reached. It is also removed once the question is
/// over; see [`Config`].
pub fn write_config(
    data_dir: &Path,
    bridge: &Path,
    port: u16,
    token: &str,
) -> std::io::Result<Config> {
    let path = data_dir.join("mcp.json");

    std::fs::create_dir_all(data_dir)?;
    std::fs::write(
        &path,
        serde_json::to_vec_pretty(&config(bridge, port, token))?,
    )?;

    Ok(Config(path))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The description and the schema are what decide whether a tool is
    /// reached for at the right moment. A shape that carries the name and
    /// drops those is a tool nothing can call correctly.
    #[test]
    fn the_mcp_shape_carries_what_a_client_needs_to_call_one() {
        for tool in crate::ai::tools::as_mcp().as_array().unwrap() {
            let name = tool["name"].as_str().unwrap_or_default();

            assert!(!name.is_empty(), "a tool went out with no name");
            assert!(
                tool["description"].as_str().is_some_and(|d| d.len() > 30),
                "{name} has no useful description",
            );
            assert_eq!(
                tool["inputSchema"]["type"],
                json!("object"),
                "{name} has no object schema",
            );
        }
    }

    mod what_is_allowed {
        use super::*;

        /// A rule per tool rather than one for the whole server. Both work
        /// today; only this one stops a server that later grew something
        /// nobody meant to allow from being allowed by an old rule.
        #[test]
        fn every_tool_is_named_and_nothing_else_is() {
            let allowed = allowed();

            assert_eq!(allowed.len(), crate::ai::tools::CATALOGUE.len());

            for tool in crate::ai::tools::CATALOGUE {
                let expected = format!("mcp__sill__{}", tool.name);
                assert!(allowed.contains(&expected), "{expected} is not allowed");
            }
        }

        /// The name that comes back on the stream is the namespaced one,
        /// and the window's word table is keyed by the plain one.
        #[test]
        fn a_sill_tool_loses_its_server_prefix() {
            assert_eq!(short_name("mcp__sill__list_windows"), "list_windows");
            assert_eq!(short_name("Bash"), "Bash");
            assert_eq!(short_name("mcp__other__read"), "mcp__other__read");

            for name in allowed() {
                let plain = short_name(&name);
                assert!(
                    crate::ai::tools::CATALOGUE
                        .iter()
                        .any(|tool| tool.name == plain),
                    "{name} does not shorten to a tool: {plain}",
                );
            }
        }

        /// The prefix is what the permission rule matches on, and it comes
        /// from the name the handshake gives. If those two ever disagree,
        /// every rule silently matches nothing and every tool is denied.
        #[test]
        fn the_prefix_is_the_name_the_handshake_gives() {
            for name in allowed() {
                assert!(
                    name.starts_with(&format!("mcp__{}__", protocol::SERVER)),
                    "{name} is not namespaced by the server name",
                );
            }
        }
    }

    mod the_config {
        use super::*;

        #[test]
        fn it_names_the_bridge_the_port_and_the_secret() {
            let written = config(Path::new("C:/Sill/sill.exe"), 51234, "s3cret");
            let server = &written["mcpServers"]["sill"];

            assert_eq!(server["type"], json!("stdio"));
            assert_eq!(server["command"], json!("C:/Sill/sill.exe"));
            assert_eq!(server["args"], json!(["--mcp"]));
            assert_eq!(server["env"]["SILL_MCP_PORT"], json!("51234"));
            assert_eq!(server["env"]["SILL_MCP_TOKEN"], json!("s3cret"));
        }

        /// The flag in the config has to be the flag the process looks for.
        /// Two spellings would mean a bridge that starts as the launcher, and
        /// on a machine already running one that means the single instance
        /// plugin toggling the launcher window every time a question is asked.
        #[test]
        fn the_argument_is_the_one_the_process_answers_to() {
            let written = config(Path::new("sill.exe"), 1, "x");
            let args = written["mcpServers"]["sill"]["args"].as_array().unwrap();

            assert_eq!(args, &[json!(FLAG)]);
        }

        /// It must not land in the directory the CLI runs from, which is empty
        /// so that a session picks up no hooks and no servers of its own.
        #[test]
        fn it_is_not_written_where_the_cli_runs() {
            let data = std::env::temp_dir().join("sill-mcp-config-test");
            let _ = std::fs::remove_dir_all(&data);

            let written = write_config(&data, Path::new("sill.exe"), 7, "x").unwrap();
            let neutral = crate::ai::claude_code::neutral_directory(&data);

            assert!(
                !written.path().starts_with(&neutral),
                "{:?} is inside {neutral:?}",
                written.path()
            );
            assert!(written.path().is_file());

            let _ = std::fs::remove_dir_all(&data);
        }

        /// The secret does not outlive the question that needed it.
        ///
        /// Anything holding this token can read a file, the clipboard, the
        /// screen and the selection without a card being raised, so a copy
        /// left in the app data folder is a copy in every backup of it.
        #[test]
        fn the_token_is_taken_with_it() {
            let data = std::env::temp_dir().join("sill-mcp-config-lifetime");
            let _ = std::fs::remove_dir_all(&data);

            let path = {
                let written = write_config(&data, Path::new("sill.exe"), 7, "the-secret").unwrap();
                let path = written.path().to_path_buf();

                let held = std::fs::read_to_string(&path).unwrap();
                assert!(held.contains("the-secret"), "it was never written");

                path
            };

            assert!(
                !path.exists(),
                "the config outlived the run it was written for, so the token \
                 is still readable at {path:?}"
            );

            let _ = std::fs::remove_dir_all(&data);
        }
    }
}
