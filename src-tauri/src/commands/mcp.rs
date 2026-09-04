//! MCP servers, as the settings panel needs them.
//!
//! One adapter, and it is thin. Everything that decides lives in
//! [`crate::ai::mcp::client`]: the deadline, the handshake, what happens to the
//! process afterwards.
//!
//! Nothing here caches a list and there is nowhere for one to live. The server
//! is asked when somebody presses Check, and the process it starts is gone
//! before this returns.

use crate::ai::mcp::client::{self, Program, Tool};

/**
Asks one server what it can do, so somebody can pick a tool without reading its
source.

**This is the only place in Sill a server is started by anything other than
somebody running one of its actions.** It is a button, pressed by a person
looking at the panel, and the whole exchange is bounded by
[`client::STARTING`]: a server that never answers ends as a sentence under the
field rather than a settings window that will not close.

Not called on mount, and that is the point. Opening the settings panel with
five servers configured must not start five programs; opening it costs reading
what is already in the preferences, and the person asks about the one they are
working on.
*/
#[tauri::command]
pub(crate) async fn mcp_tools(
    name: String,
    command: String,
    args: Vec<String>,
) -> Result<Vec<Tool>, String> {
    if command.trim().is_empty() {
        return Err("Name the program that starts this server first.".to_string());
    }

    // The form as it is on screen rather than what was last saved, because the
    // whole reason to press this is to find out whether what was just typed
    // works before saving it.
    client::tools(Program {
        name: if name.trim().is_empty() {
            "That server"
        } else {
            name.trim()
        },
        command: command.trim(),
        args: &args,
    })
    .await
}
