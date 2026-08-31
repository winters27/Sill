// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    /*
     * `sill.exe --mcp` is not the launcher.
     *
     * It is the bridge an MCP client starts, which connects back to the
     * running Sill and copies bytes. Answered here, before anything Tauri
     * does: the single instance plugin turns a second launch into a toggle of
     * the launcher window, and a bridge that reached it would flash somebody's
     * launcher on screen every time a question was asked.
     */
    if sill_lib::ai::mcp::link::asked_for_bridge() {
        std::process::exit(sill_lib::ai::mcp::link::bridge());
    }

    sill_lib::run()
}
