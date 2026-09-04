//! Drives a real MCP server somebody else wrote.
//!
//! `tests/mcp_client.rs` proves the plumbing against a fixture this repository
//! controls, which is what a build agent can run. This is the other half:
//! a genuine published server, fetched and started the way somebody setting one
//! up in Settings would, so the answer is about MCP rather than about a fixture
//! written to match the client.
//!
//! It is `#[ignore]`, like every probe here, because it needs the network the
//! first time `npx` runs and a machine with Node on the path.
//!
//! ```text
//! cargo test --manifest-path src-tauri/Cargo.toml --test probes \
//!   probe_mcp -- --ignored --nocapture
//! ```

use std::time::Instant;

use sill_lib::ai::mcp::client::{self, Program};

/// Where the server is pointed. Its own scratch directory, never anything real.
fn scratch() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join("sill-mcp-probe");
    std::fs::create_dir_all(&dir).expect("a scratch directory");
    std::fs::write(dir.join("notes.md"), "# Notes\n\nBuy milk.\n").expect("a file to read");
    dir
}

/// The filesystem server from the reference implementations, end to end.
///
/// Asked what it has, then asked to read a file, with the clock running on
/// both. What is worth reading in the output is the second number: the whole
/// cost of an MCP action is a process start, and this is what it actually is
/// on this machine.
#[test]
#[ignore]
fn probe_mcp_real_filesystem_server() {
    let dir = scratch();
    let args = vec![
        "-y".to_string(),
        "@modelcontextprotocol/server-filesystem".to_string(),
        dir.to_string_lossy().to_string(),
    ];

    let program = Program {
        name: "filesystem",
        command: "npx.cmd",
        args: &args,
    };

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("a runtime");

    let started = Instant::now();
    let offered = runtime
        .block_on(client::tools(program))
        .expect("the server lists its tools");
    println!("tools/list took {:?}", started.elapsed());

    println!("{} tools:", offered.len());
    for tool in &offered {
        println!(
            "  {:<28} {}",
            tool.name,
            tool.description.lines().next().unwrap_or_default()
        );
    }

    let started = Instant::now();
    let said = runtime
        .block_on(client::call(
            program,
            "read_text_file",
            serde_json::json!({ "path": dir.join("notes.md").to_string_lossy() }),
        ))
        .expect("the tool ran");
    println!("tools/call took {:?}", started.elapsed());
    println!("--- what it said ---\n{said}\n--------------------");

    assert!(said.contains("Buy milk"), "{said}");

    // And a tool it does not have is refused rather than hung on.
    let refused = runtime
        .block_on(client::call(program, "make_coffee", serde_json::json!({})))
        .expect_err("there is no such tool");
    println!("a tool it does not have: {refused}");
}

/**
The same server, started by its own entry point rather than through `npx`.

**What this measures is the difference between the two, and it is most of the
cost.** `npx` resolves a package name on every start, which on Windows is
seconds; naming `node` and the file directly is a Node start and nothing else.
Both are a per-call cost by design, so the number a person actually pays
depends on which of the two they put in the Command field, and Settings should
say so rather than leave them wondering why one server feels slow.

Set `SILL_MCP_PROBE_ENTRY` to the server's `dist/index.js`. Skipped rather than
failed without it: there is no path that is right on more than one machine.

```text
SILL_MCP_PROBE_ENTRY=...\server-filesystem\dist\index.js \
  cargo test --manifest-path src-tauri/Cargo.toml --test probes \
  probe_mcp_direct -- --ignored --nocapture
```
*/
#[test]
#[ignore]
fn probe_mcp_direct_entry_point() {
    let Ok(entry) = std::env::var("SILL_MCP_PROBE_ENTRY") else {
        println!("SILL_MCP_PROBE_ENTRY is not set, so there is nothing to start");
        return;
    };

    let dir = scratch();
    let args = vec![entry, dir.to_string_lossy().to_string()];

    let program = Program {
        name: "filesystem",
        command: "node",
        args: &args,
    };

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("a runtime");

    for round in 1..=3 {
        let started = Instant::now();
        let said = runtime
            .block_on(client::call(
                program,
                "read_text_file",
                serde_json::json!({ "path": dir.join("notes.md").to_string_lossy() }),
            ))
            .expect("the tool ran");

        println!("round {round}: {:?}", started.elapsed());
        assert!(said.contains("Buy milk"), "{said}");
    }
}
