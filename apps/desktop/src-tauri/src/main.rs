// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // MCP child mode must be checked before any GUI/Tauri initialization:
    // agents spawn `lumina-app.exe --lumina-mcp` as a stdio MCP server, and
    // the GUI writes ANSI logs to stdout that corrupt the protocol stream.
    lumina_mcp::run_if_invoked();
    lumina_lib::run()
}
