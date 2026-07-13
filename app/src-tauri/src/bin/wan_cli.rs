// Headless CLI entrypoint sharing all RouterOS logic with the Tauri app
// (via the wan_monitor_app_lib rlib) — spawned by app/src/tool/transport-cli.js
// so the Node MCP/CLI process (bin/wan-monitor.mjs) can dispatch the same
// tools the in-app chat panel uses, without needing Tauri's invoke() bridge.
// Usage: wan-cli <tool-name> [json-input]

use wan_monitor_app_lib::{
    measure_now_impl, read_router_log_impl, read_wan_speed_impl, restore_failover_config_impl, toggle_wan_impl,
};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let tool = args.get(1).map(|s| s.as_str()).unwrap_or("");
    let input: serde_json::Value =
        args.get(2).and_then(|s| serde_json::from_str(s).ok()).unwrap_or(serde_json::Value::Null);

    let result: Result<String, String> = match tool {
        "measure_now" => Ok(measure_now_impl()),
        "read_wan_speed" => read_wan_speed_impl(),
        "read_router_log" => read_router_log_impl(),
        "toggle_wan" => {
            let channel = input.get("channel").and_then(|v| v.as_str()).unwrap_or_default().to_string();
            let on = input.get("on").and_then(|v| v.as_bool()).unwrap_or(false);
            toggle_wan_impl(channel, on)
        }
        "restore_failover_config" => restore_failover_config_impl(),
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
