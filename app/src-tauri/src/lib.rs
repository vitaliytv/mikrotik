mod routeros;

use routeros::{connect_and_login, load_config, read_traffic, ApiRos};
use serde::Serialize;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

const MONITOR_INTERVAL: Duration = Duration::from_secs(15);
const WAN_SPEED_TEST_URL: &str = "https://speed.cloudflare.com/__down?bytes=50000000";

fn active_wan_state(api: &mut ApiRos) -> Result<Option<String>, String> {
    let clients = api.talk(&["/ip/dhcp-client/print"]).map_err(|e| e.to_string())?;
    for (reply, attrs) in clients {
        if reply != "!re" { continue; }
        let is_active = attrs.get("=default-route-tables").map(String::as_str).is_some_and(|tables| tables.contains("main:1"));
        if !is_active { continue; }
        match attrs.get("=name").map(String::as_str) {
            Some("client1") => return Ok(Some("bite".to_string())),
            Some("client2") => return Ok(Some("lmt".to_string())),
            _ => {}
        }
    }
    Ok(None)
}

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

#[derive(Serialize)]
struct WanSpeedMeasurement {
    channel: String,
    interface: String,
    megabits_per_second: f64,
    downloaded_bytes: u64,
    duration_ms: u128,
}

#[derive(Serialize)]
struct WanSpeedTestResult {
    tested_at: String,
    measurements: Vec<WanSpeedMeasurement>,
}

fn routeros_value<'a>(
    rows: &'a [(String, std::collections::HashMap<String, String>)],
    key: &str,
) -> Option<&'a str> {
    rows.iter()
        .rev()
        .find_map(|(_, attrs)| attrs.get(key).map(String::as_str))
}

fn wan_interface_address(api: &mut ApiRos, interface: &str) -> Result<String, String> {
    let rows = api
        .talk(&[
            "/ip/address/print",
            "=detail=",
            &format!("?interface={interface}"),
        ])
        .map_err(|error| error.to_string())?;
    rows.into_iter()
        .find_map(|(reply, attrs)| {
            (reply == "!re")
                .then(|| attrs.get("=address"))
                .flatten()
                .and_then(|address| {
                    address
                        .split_once('/')
                        .map(|(address, _)| address.to_string())
                })
        })
        .ok_or_else(|| format!("RouterOS не має IPv4-адреси на {interface}"))
}

fn remove_router_file(api: &mut ApiRos, file_name: &str) {
    let Ok(rows) = api.talk(&["/file/print", "=detail="]) else {
        return;
    };
    for (reply, attrs) in rows {
        if reply == "!re" && attrs.get("=name").map(String::as_str) == Some(file_name) {
            if let Some(id) = attrs.get("=.id") {
                let _ = api.talk(&["/file/remove", &format!("=numbers={id}")]);
            }
        }
    }
}

fn run_wan_speed_measurement(
    api: &mut ApiRos,
    channel: &str,
    interface: &str,
) -> Result<WanSpeedMeasurement, String> {
    let source_address = wan_interface_address(api, interface)?;
    let file_name = format!(
        "mymikrotik-speed-{channel}-{}.tmp",
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
    );
    let started = std::time::Instant::now();
    let result = api.talk(&[
        "/tool/fetch",
        &format!("=url={WAN_SPEED_TEST_URL}"),
        &format!("=src-address={source_address}"),
        "=output=file",
        &format!("=dst-path={file_name}"),
    ]);
    let duration = started.elapsed();
    let result = result.map_err(|error| error.to_string());
    remove_router_file(api, &file_name);
    let rows = result?;
    if let Some(message) = routeros_value(&rows, "=message") {
        return Err(format!("{channel}: {message}"));
    }
    let downloaded_kib = routeros_value(&rows, "=downloaded")
        .ok_or_else(|| format!("{channel}: RouterOS не повернув обсяг завантаження"))?
        .parse::<u64>()
        .map_err(|error| format!("{channel}: некоректний лічильник завантаження: {error}"))?;
    let downloaded_bytes = downloaded_kib * 1024;
    if downloaded_bytes == 0 || duration.is_zero() {
        return Err(format!("{channel}: тест не передав дані"));
    }
    Ok(WanSpeedMeasurement {
        channel: channel.to_string(),
        interface: interface.to_string(),
        megabits_per_second: downloaded_bytes as f64 * 8.0 / duration.as_secs_f64() / 1_000_000.0,
        downloaded_bytes,
        duration_ms: duration.as_millis(),
    })
}

pub fn run_wan_speed_test_impl() -> Result<String, String> {
    let mut api = connect_and_login(Duration::from_secs(120))?;
    let device_mode = api
        .talk(&["/system/device-mode/print"])
        .map_err(|error| error.to_string())?;
    if routeros_value(&device_mode, "=fetch") != Some("true") {
        return Err("RouterOS Device Mode вимикає fetch; увімкніть fetch перед тестом".to_string());
    }
    let measurements = ["LMT", "BITE"]
        .into_iter()
        .zip(["ether3", "ether1"])
        .map(|(channel, interface)| run_wan_speed_measurement(&mut api, channel, interface))
        .collect::<Result<Vec<_>, _>>()?;
    serde_json::to_string(&WanSpeedTestResult {
        tested_at: now_iso(),
        measurements,
    })
    .map_err(|error| error.to_string())
}

#[tauri::command]
fn run_wan_speed_test() -> Result<String, String> {
    run_wan_speed_test_impl()
}

// ---------- швидка діагностика доступності RouterOS ----------

#[derive(Serialize)]
struct DiagnosticSnapshot {
    checked_at: String,
    endpoint: String,
    api_reachable: bool,
    latency_ms: Option<u128>,
    error: String,
    identity: String,
    scheduler_enabled: String,
    scheduler_runs: String,
    scheduler_last_started: String,
    scheduler_on_event: String,
    scheduler_policy: String,
    controller_state: String,
    lmt_bad_cycles: String,
    last_decision: String,
    script_invalid: String,
    script_runs: String,
    script_last_started: String,
    script_jobs: Vec<String>,
}

pub fn read_router_diagnostic_impl() -> Result<String, String> {
    let checked_at = now_iso();
    let config = match load_config() {
        Ok(config) => config,
        Err(error) => {
            return serde_json::to_string(&DiagnosticSnapshot {
                checked_at,
                endpoint: "192.168.88.1:8728".to_string(),
                api_reachable: false,
                latency_ms: None,
                error,
                identity: String::new(),
                scheduler_enabled: "unknown".to_string(),
                scheduler_runs: String::new(),
                scheduler_last_started: String::new(),
                scheduler_on_event: String::new(),
                scheduler_policy: String::new(),
                controller_state: "unknown".to_string(),
                lmt_bad_cycles: String::new(),
                last_decision: String::new(),
                script_invalid: "unknown".to_string(),
                script_runs: String::new(),
                script_last_started: String::new(),
                script_jobs: Vec::new(),
            })
            .map_err(|e| e.to_string());
        }
    };
    let endpoint = format!("{}:8728", config.host);
    let started = std::time::Instant::now();
    let mut api = match connect_and_login(Duration::from_secs(3)) {
        Ok(api) => api,
        Err(error) => {
            return serde_json::to_string(&DiagnosticSnapshot {
                checked_at,
                endpoint,
                api_reachable: false,
                latency_ms: Some(started.elapsed().as_millis()),
                error,
                identity: String::new(),
                scheduler_enabled: "unknown".to_string(),
                scheduler_runs: String::new(),
                scheduler_last_started: String::new(),
                scheduler_on_event: String::new(),
                scheduler_policy: String::new(),
                controller_state: "unknown".to_string(),
                lmt_bad_cycles: String::new(),
                last_decision: String::new(),
                script_invalid: "unknown".to_string(),
                script_runs: String::new(),
                script_last_started: String::new(),
                script_jobs: Vec::new(),
            })
            .map_err(|e| e.to_string());
        }
    };

    let identity = api
        .talk(&["/system/identity/print"])
        .ok()
        .into_iter()
        .flatten()
        .find_map(|(reply, attrs)| (reply == "!re").then(|| attrs.get("=name").cloned()).flatten())
        .unwrap_or_default();
    let active_wan_state = active_wan_state(&mut api).ok().flatten();
    let globals: std::collections::HashMap<String, String> = api
        .talk(&["/system/script/environment/print"])
        .ok()
        .into_iter()
        .flatten()
        .filter_map(|(reply, attrs)| {
            (reply == "!re").then(|| Some((attrs.get("=name")?.clone(), attrs.get("=value").cloned().unwrap_or_default())))?
        })
        .collect();
    let mut snapshot = DiagnosticSnapshot {
        checked_at,
        endpoint,
        api_reachable: true,
        latency_ms: Some(started.elapsed().as_millis()),
        error: String::new(),
        identity,
        scheduler_enabled: "missing".to_string(),
        scheduler_runs: String::new(),
        scheduler_last_started: String::new(),
        scheduler_on_event: String::new(),
        scheduler_policy: String::new(),
        controller_state: active_wan_state.clone().unwrap_or_else(|| "unknown".to_string()),
        lmt_bad_cycles: globals.get("dwActiveBad").or_else(|| globals.get("dwLmtBad")).cloned().unwrap_or_default(),
        last_decision: active_wan_state.unwrap_or_default(),
        script_invalid: "missing".to_string(),
        script_runs: String::new(),
        script_last_started: String::new(),
        script_jobs: Vec::new(),
    };
    if let Ok(rows) = api.talk(&["/system/scheduler/print"]) {
        for (reply, attrs) in rows {
            if reply == "!re" && attrs.get("=name").map(String::as_str) == Some("DUALWAN-health-every-5s") {
                snapshot.scheduler_enabled = (!matches!(attrs.get("=disabled").map(String::as_str), Some("true"))).to_string();
                snapshot.scheduler_runs = attrs.get("=run-count").cloned().unwrap_or_default();
                snapshot.scheduler_last_started = attrs.get("=last-started").cloned().unwrap_or_default();
                snapshot.scheduler_on_event = attrs.get("=on-event").cloned().unwrap_or_default();
                snapshot.scheduler_policy = attrs.get("=policy").cloned().unwrap_or_default();
            }
        }
    }
    if let Ok(rows) = api.talk(&["/system/script/print"]) {
        for (reply, attrs) in rows {
            if reply == "!re" && attrs.get("=name").map(String::as_str) == Some("DUALWAN-health") {
                snapshot.script_invalid = attrs.get("=invalid").cloned().unwrap_or_default();
                snapshot.script_runs = attrs.get("=run-count").cloned().unwrap_or_default();
                snapshot.script_last_started = attrs.get("=last-started").cloned().unwrap_or_default();
            }
        }
    }
    if let Ok(rows) = api.talk(&["/system/script/job/print"]) {
        snapshot.script_jobs = rows
            .into_iter()
            .filter_map(|(reply, attrs)| (reply == "!re").then(|| attrs.get("=script").cloned()).flatten())
            .collect();
    }
    serde_json::to_string(&snapshot).map_err(|e| e.to_string())
}

#[tauri::command]
fn read_router_diagnostic() -> Result<String, String> {
    read_router_diagnostic_impl()
}

// ---------- стан router-local dual-WAN controller ----------

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
    switch_events: Vec<SwitchEvent>,
    raw_log: Vec<RawLogLine>,
    log_total_lines: usize,
}

fn controller_state(api: &mut ApiRos) -> String {
    active_wan_state(api)
        .ok()
        .flatten()
        .unwrap_or_else(|| "unknown".to_string())
}

pub fn read_router_log_impl() -> Result<String, String> {
    // The UI retries unavailable RouterOS endpoints. Keep each attempt short
    // so an offline router cannot make startup appear frozen.
    let mut api = connect_and_login(Duration::from_secs(5))?;

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
    let mut seen_switches = std::collections::HashSet::new();
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
        switch_events,
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
            if let Some(window) = app.get_webview_window("main") {
                let base_title = window.title().unwrap_or_default();
                let version = &app.package_info().version;
                let _ = window.set_title(&format!("{base_title} v{version}"));
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            read_wan_speed,
            run_wan_speed_test,
            read_router_log,
            read_router_diagnostic
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
