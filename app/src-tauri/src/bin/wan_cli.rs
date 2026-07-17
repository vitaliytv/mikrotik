// Headless CLI entrypoint sharing all RouterOS logic with the Tauri app
// (via the wan_monitor_app_lib rlib) — spawned by app/src/tool/transport-cli.js
// so the Node MCP/CLI process (bin/wan-monitor.mjs) can dispatch the same
// tools the in-app chat panel uses, without needing Tauri's invoke() bridge.
// Usage: wan-cli <tool-name> [json-input]

use wan_monitor_app_lib::{read_router_diagnostic_impl, read_router_log_impl, read_wan_speed_impl};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let tool = args.get(1).map(|s| s.as_str()).unwrap_or("");
    let result: Result<String, String> = match tool {
        "read_wan_speed" => read_wan_speed_impl(),
        "read_router_log" => read_router_log_impl(),
        "read_router_diagnostic" => read_router_diagnostic_impl(),
        other => Err(format!("Unknown tool: {}", other)),
    };

    match result {
        Ok(text) => println!("{}", text),
        Err(text) => {
            eprintln!("{}", text);
            std::process::exit(1);
        }
    }
}
