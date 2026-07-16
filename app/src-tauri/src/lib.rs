mod routeros;

use routeros::{connect_and_login, read_traffic, ApiRos, PROBE_LMT_PUBLIC, PROBE_ZTE};
use serde::Serialize;
use std::time::Duration;
use tauri::{AppHandle, Emitter};

const MONITOR_INTERVAL: Duration = Duration::from_secs(15);

#[derive(Serialize, Clone)]
struct WanSample {
    ts: String,
    zte_rx_bps: Option<i64>,
    zte_tx_bps: Option<i64>,
    soyea_rx_bps: Option<i64>,
    soyea_tx_bps: Option<i64>,
}

fn now_iso() -> String {
    chrono::Local::now().format("%Y-%m-%dT%H:%M:%S").to_string()
}

fn traffic_sample(api: &mut ApiRos, ts: String) -> WanSample {
    let (zte_rx_bps, zte_tx_bps, soyea_rx_bps, soyea_tx_bps) = read_traffic(api);
    WanSample {
        ts,
        zte_rx_bps,
        zte_tx_bps,
        soyea_rx_bps,
        soyea_tx_bps,
    }
}

fn traffic_sample_reuse(api_slot: &mut Option<ApiRos>) -> Option<WanSample> {
    let ts = now_iso();
    let alive = api_slot
        .as_mut()
        .map(|api| api.talk(&["/system/identity/print"]).is_ok())
        .unwrap_or(false);
    if !alive {
        *api_slot = connect_and_login(Duration::from_secs(8)).ok();
    }
    api_slot.as_mut().map(|api| traffic_sample(api, ts))
}

fn start_monitor_thread(app: AppHandle) {
    std::thread::spawn(move || {
        let mut api: Option<ApiRos> = None;
        loop {
            let start = std::time::Instant::now();
            if let Some(sample) = traffic_sample_reuse(&mut api) {
                let _ = app.emit("wan-sample", &sample);
            }
            let elapsed = start.elapsed();
            if elapsed < MONITOR_INTERVAL {
                std::thread::sleep(MONITOR_INTERVAL - elapsed);
            }
        }
    });
}

// `#[tauri::command]` doesn't tolerate a `pub fn` (duplicate macro-namespace
// item errors), so each command is a thin private wrapper around a plain
// `pub fn ..._impl` that src/bin/wan_cli.rs (the headless CLI/MCP entrypoint)
// calls directly via the `wan_monitor_app_lib` rlib.

pub fn read_wan_speed_impl() -> Result<String, String> {
    let mut api = connect_and_login(Duration::from_secs(10))?;
    let (zte_rx, zte_tx, soyea_rx, soyea_tx) = read_traffic(&mut api);
    let mut obj = serde_json::json!({ "ts": now_iso() });
    if let (Some(rx), Some(tx)) = (zte_rx, zte_tx) {
        obj["zte"] = serde_json::json!({ "rx_bps": rx, "tx_bps": tx });
    }
    if let (Some(rx), Some(tx)) = (soyea_rx, soyea_tx) {
        obj["soyea"] = serde_json::json!({ "rx_bps": rx, "tx_bps": tx });
    }
    Ok(obj.to_string())
}

#[tauri::command]
fn read_wan_speed() -> Result<String, String> {
    read_wan_speed_impl()
}

// ---------- стан router-local dual-WAN controller ----------

#[derive(Serialize)]
struct ProbeInfo {
    channel: String,
    target: String,
    received: String,
    loss_percent: String,
    avg_rtt: String,
}

#[derive(Serialize)]
struct RouteInfo {
    channel: String,
    table: String,
    distance: String,
    active: String,
    gateway: String,
}

#[derive(Serialize)]
struct DhcpInfo {
    channel: String,
    status: String,
    address: String,
    gateway: String,
    default_route_tables: String,
}

#[derive(Serialize)]
struct ControllerInfo {
    scheduler_enabled: String,
    interval: String,
    scheduler_runs: String,
    script_invalid: String,
    script_runs: String,
    state: String,
}

#[derive(Serialize)]
struct SwitchEvent {
    time: String,
    state: String,
    reason: String,
}

#[derive(Serialize)]
struct QualityEvent {
    time: String,
    status: String,
}

#[derive(Serialize)]
struct RawLogLine {
    time: String,
    topics: String,
    message: String,
}

#[derive(Serialize)]
struct RouterLogResult {
    controller: ControllerInfo,
    dhcp: Vec<DhcpInfo>,
    routes: Vec<RouteInfo>,
    probes: Vec<ProbeInfo>,
    switch_events: Vec<SwitchEvent>,
    quality_events: Vec<QualityEvent>,
    raw_log: Vec<RawLogLine>,
    log_total_lines: usize,
}

fn controller_state(api: &mut ApiRos) -> String {
    api.talk(&["/system/script/environment/print"])
        .ok()
        .into_iter()
        .flatten()
        .find_map(|(reply, attrs)| {
            (reply == "!re" && attrs.get("=name").map(String::as_str) == Some("dwState"))
                .then(|| attrs.get("=value").cloned().unwrap_or_default())
        })
        .unwrap_or_else(|| "unknown".to_string())
}

fn read_probe(api: &mut ApiRos, channel: &str, address: &str) -> ProbeInfo {
    let summary = api
        .talk(&[
            "/ping",
            &format!("=address={}", address),
            "=count=3",
            "=interval=200ms",
        ])
        .ok()
        .into_iter()
        .flatten()
        .filter(|(reply, _)| reply == "!re")
        .last()
        .map(|(_, attrs)| attrs)
        .unwrap_or_default();
    ProbeInfo {
        channel: channel.to_string(),
        target: address.to_string(),
        received: summary.get("=received").cloned().unwrap_or_else(|| "0".to_string()),
        loss_percent: summary.get("=packet-loss").cloned().unwrap_or_else(|| "100".to_string()),
        avg_rtt: summary.get("=avg-rtt").cloned().unwrap_or_else(|| "?".to_string()),
    }
}

pub fn read_router_log_impl() -> Result<String, String> {
    let mut api = connect_and_login(Duration::from_secs(20))?;

    let state = controller_state(&mut api);
    let mut controller = ControllerInfo {
        scheduler_enabled: "unknown".to_string(),
        interval: String::new(),
        scheduler_runs: String::new(),
        script_invalid: "unknown".to_string(),
        script_runs: String::new(),
        state,
    };
    if let Ok(rows) = api.talk(&["/system/scheduler/print"]) {
        for (r, attrs) in rows {
            if r == "!re" && attrs.get("=name").map(String::as_str) == Some("DUALWAN-health-every-5s") {
                controller.scheduler_enabled = (!matches!(attrs.get("=disabled").map(String::as_str), Some("true"))).to_string();
                controller.interval = attrs.get("=interval").cloned().unwrap_or_default();
                controller.scheduler_runs = attrs.get("=run-count").cloned().unwrap_or_default();
            }
        }
    }
    if let Ok(rows) = api.talk(&["/system/script/print"]) {
        for (r, attrs) in rows {
            if r == "!re" && attrs.get("=name").map(String::as_str) == Some("DUALWAN-health") {
                controller.script_invalid = attrs.get("=invalid").cloned().unwrap_or_default();
                controller.script_runs = attrs.get("=run-count").cloned().unwrap_or_default();
            }
        }
    }

    let mut dhcp = Vec::new();
    if let Ok(rows) = api.talk(&["/ip/dhcp-client/print"]) {
        for (r, attrs) in rows {
            if r != "!re" { continue; }
            let interface = attrs.get("=interface").map(String::as_str).unwrap_or_default();
            let channel = match interface { "ether3" => "zte", "ether1" => "soyea", _ => continue };
            dhcp.push(DhcpInfo {
                channel: channel.to_string(),
                status: attrs.get("=status").cloned().unwrap_or_default(),
                address: attrs.get("=address").cloned().unwrap_or_default(),
                gateway: attrs.get("=gateway").cloned().unwrap_or_default(),
                default_route_tables: attrs.get("=default-route-tables").cloned().unwrap_or_default(),
            });
        }
    }

    let mut routes = Vec::new();
    if let Ok(rows) = api.talk(&["/ip/route/print"]) {
        for (r, attrs) in rows {
            if r != "!re" {
                continue;
            }
            if attrs.get("=dynamic").map(String::as_str) == Some("true")
                && attrs.get("=dhcp").map(String::as_str) == Some("true")
                && attrs.get("=dst-address").map(String::as_str) == Some("0.0.0.0/0") {
                let channel = match attrs.get("=gateway").map(String::as_str) {
                    Some("192.168.0.1") => "zte",
                    Some("192.168.8.1") => "soyea",
                    _ => "?",
                };
                routes.push(RouteInfo {
                    channel: channel.to_string(),
                    table: attrs.get("=routing-table").cloned().unwrap_or_else(|| "main".to_string()),
                    distance: attrs.get("=distance").cloned().unwrap_or_default(),
                    active: attrs.get("=active").cloned().unwrap_or_default(),
                    gateway: attrs.get("=gateway").cloned().unwrap_or_default(),
                });
            }
        }
    }

    let log_rows = api.talk(&["/log/print"]).map_err(|e| e.to_string())?;
    let log_rows: Vec<_> = log_rows.into_iter().filter(|(r, _)| r == "!re").collect();

    let mut switch_events = Vec::new();
    let mut quality_events = Vec::new();
    let mut seen_switches = std::collections::HashSet::new();
    let mut seen_quality = std::collections::HashSet::new();
    for (_, attrs) in &log_rows {
        let msg = attrs.get("=message").cloned().unwrap_or_default();
        let t = attrs.get("=time").cloned().unwrap_or_default();
        if let Some(rest) = msg.strip_prefix("DUALWAN state=") {
            let mut parts = rest.split_whitespace();
            let state = parts.next().unwrap_or_default().to_string();
            let reason = parts
                .find_map(|part| part.strip_prefix("reason="))
                .unwrap_or_default()
                .to_string();
            let key = (t.clone(), state.clone(), reason.clone());
            if !seen_switches.insert(key) {
                continue;
            }
            switch_events.push(SwitchEvent {
                time: t,
                state,
                reason,
            });
        } else if let Some(rest) = msg.strip_prefix("DUALWAN quality=") {
            let status = rest.split_whitespace().next().unwrap_or_default().to_string();
            let key = (t.clone(), status.clone());
            if seen_quality.insert(key) {
                quality_events.push(QualityEvent { time: t, status });
            }
        }
    }

    let log_total_lines = log_rows.len();
    let raw_log: Vec<RawLogLine> = log_rows
        .iter()
        .map(|(_, attrs)| RawLogLine {
            time: attrs.get("=time").cloned().unwrap_or_default(),
            topics: attrs.get("=topics").cloned().unwrap_or_default(),
            message: attrs.get("=message").cloned().unwrap_or_default(),
        })
        .collect();
    let raw_log_len = raw_log.len();
    let raw_log: Vec<RawLogLine> = raw_log
        .into_iter()
        .skip(raw_log_len.saturating_sub(300))
        .collect();
    let result = RouterLogResult {
        controller,
        dhcp,
        routes,
        probes: vec![
            read_probe(&mut api, "zte", PROBE_ZTE),
            read_probe(&mut api, "zte", PROBE_LMT_PUBLIC),
        ],
        switch_events,
        quality_events,
        raw_log,
        log_total_lines,
    };
    serde_json::to_string(&result).map_err(|e| e.to_string())
}

#[tauri::command]
fn read_router_log() -> Result<String, String> {
    read_router_log_impl()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_agent::init());

    #[cfg(desktop)]
    let builder = builder.plugin(tauri_plugin_updater::Builder::new().build());

    // relaunch() after installing the update, so the app restarts into the
    // new version on its own instead of waiting for a manual restart.
    let builder = builder.plugin(tauri_plugin_process::init());

    #[cfg(debug_assertions)]
    let builder = builder.plugin(tauri_plugin_mcp_bridge::init());

    builder
        .setup(|app| {
            start_monitor_thread(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![read_wan_speed, read_router_log])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
