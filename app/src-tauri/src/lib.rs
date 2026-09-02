mod routeros;

use routeros::{connect_and_login, load_config, read_traffic, ApiRos};
use regex::Regex;
use serde::Serialize;
use std::sync::LazyLock;
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
    active_bad_cycles: String,
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
                active_bad_cycles: String::new(),
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
                active_bad_cycles: String::new(),
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
        active_bad_cycles: globals.get("dwActiveBad").cloned().unwrap_or_default(),
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
    history_source: String,
    history_file_count: usize,
    raw_log: Vec<RawLogLine>,
    log_total_lines: usize,
}

#[derive(Debug, Serialize, PartialEq)]
struct WanQualitySample {
    time: String,
    wan: String,
    kind: String,
    target: String,
    status: Option<String>,
    sent: Option<u32>,
    received: Option<u32>,
    loss_percent: Option<f64>,
    min_rtt_ms: Option<f64>,
    avg_rtt_ms: Option<f64>,
    max_rtt_ms: Option<f64>,
    jitter_ms: Option<f64>,
    stdev_rtt_ms: Option<f64>,
    tcp_connect_ms: Option<f64>,
    dns_server: Option<String>,
    dns_answer: Option<String>,
    interface: Option<String>,
    running: Option<bool>,
    tx_queue_drop: Option<u64>,
    link_downs: Option<u64>,
    rx_fcs_error: Option<u64>,
    rx_align_error: Option<u64>,
    tx_collision: Option<u64>,
    active: Option<bool>,
    hard_bad_cycles: Option<u32>,
    severe_bad_cycles: Option<u32>,
    quality_bad_cycles: Option<u32>,
    last_switch_uptime_ms: Option<f64>,
}

#[derive(Serialize)]
struct WanQualityResult {
    samples: Vec<WanQualitySample>,
    history_file_count: usize,
}

fn controller_state(api: &mut ApiRos) -> String {
    active_wan_state(api)
        .ok()
        .flatten()
        .unwrap_or_else(|| "unknown".to_string())
}

fn switch_event(time: String, message: &str) -> Option<SwitchEvent> {
    let rest = message.strip_prefix("DUALWAN state=")?;
    let mut parts = rest.split_whitespace();
    let state = parts.next()?.to_lowercase();
    if !matches!(state.as_str(), "lmt" | "bite") {
        return None;
    }
    let reason = parts
        .find_map(|part| part.strip_prefix("reason="))
        .unwrap_or_default()
        .to_string();
    Some(SwitchEvent {
        time,
        state,
        reason,
    })
}

fn disk_log_time(time: &str) -> Option<String> {
    chrono::NaiveDateTime::parse_from_str(time, "%b/%d/%Y %H:%M:%S")
        .ok()
        .map(|parsed| parsed.format("%Y-%m-%d %H:%M:%S").to_string())
}

fn router_file_contents(api: &mut ApiRos, name: &str, size: usize) -> Result<String, String> {
    let mut contents = String::with_capacity(size);
    let mut offset = 0;
    while offset < size {
        let chunk_size = (size - offset).min(32_768);
        let rows = api
            .talk(&[
                "/file/read",
                &format!("=file={name}"),
                &format!("=offset={offset}"),
                &format!("=chunk-size={chunk_size}"),
            ])
            .map_err(|error| error.to_string())?;
        let chunk = rows
            .iter()
            .find(|(reply, _)| reply == "!re")
            .and_then(|(_, attrs)| attrs.get("=data"))
            .cloned()
            .unwrap_or_default();
        if chunk.is_empty() {
            return Err(format!("RouterOS returned an empty chunk for {name} at offset {offset}"));
        }
        offset += chunk.len();
        contents.push_str(&chunk);
    }
    Ok(contents)
}

fn disk_switch_events(api: &mut ApiRos) -> Result<(Vec<SwitchEvent>, usize), String> {
    let rows = api
        .talk(&["/file/print"])
        .map_err(|error| error.to_string())?;
    let mut events = Vec::new();
    let mut file_count = 0;

    for (reply, attrs) in rows {
        if reply != "!re" {
            continue;
        }
        let name = attrs.get("=name").map(String::as_str).unwrap_or_default();
        if !name.starts_with("dualwan-history.") || !name.ends_with(".txt") {
            continue;
        }
        file_count += 1;
        let size = attrs.get("=size").and_then(|value| value.parse().ok()).unwrap_or(0);
        let contents = router_file_contents(api, name, size)?;
        for line in contents.lines() {
            let Some((logged_at, message)) = line.split_once(" script,warning ") else {
                continue;
            };
            let Some(time) = disk_log_time(logged_at) else {
                continue;
            };
            if let Some(event) = switch_event(time, message) {
                events.push(event);
            }
        }
    }

    events.sort_by(|left, right| left.time.cmp(&right.time));
    events.dedup_by(|left, right| {
        left.time == right.time && left.state == right.state && left.reason == right.reason
    });
    Ok((events, file_count))
}

fn routeros_duration_ms(value: &str) -> Option<f64> {
    static CLOCK_DURATION: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"^(\d+):(\d+):(\d+(?:\.\d+)?)$").expect("valid RouterOS clock duration regex")
    });
    static DURATION: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(\d+(?:\.\d+)?)(ms|us|ns|h|m|s)").expect("valid RouterOS duration regex")
    });
    if value == "4294967295" {
        return None;
    }
    if value.bytes().all(|byte| byte.is_ascii_digit()) {
        let microseconds: f64 = value.parse().ok()?;
        return Some(microseconds / 1_000.0);
    }
    if let Some(captures) = CLOCK_DURATION.captures(value) {
        let hours: f64 = captures.get(1)?.as_str().parse().ok()?;
        let minutes: f64 = captures.get(2)?.as_str().parse().ok()?;
        let seconds: f64 = captures.get(3)?.as_str().parse().ok()?;
        return Some((hours * 3_600.0 + minutes * 60.0 + seconds) * 1_000.0);
    }
    let mut total = 0.0;
    let mut matched_bytes = 0;
    for captures in DURATION.captures_iter(value) {
        let whole = captures.get(0)?;
        let amount: f64 = captures.get(1)?.as_str().parse().ok()?;
        let multiplier = match captures.get(2)?.as_str() {
            "h" => 3_600_000.0,
            "m" => 60_000.0,
            "s" => 1_000.0,
            "ms" => 1.0,
            "us" => 0.001,
            "ns" => 0.000_001,
            _ => return None,
        };
        total += amount * multiplier;
        matched_bytes += whole.as_str().len();
    }
    (matched_bytes == value.len() && matched_bytes > 0).then_some(total)
}

fn wan_quality_sample(time: String, message: &str) -> Option<WanQualitySample> {
    let fields: std::collections::HashMap<&str, &str> = message
        .strip_prefix("WANQUALITY ")?
        .split_whitespace()
        .filter_map(|field| field.split_once('='))
        .collect();
    let wan = fields.get("wan")?.to_lowercase();
    if !matches!(wan.as_str(), "lmt" | "bite") {
        return None;
    }
    let kind = fields.get("type").copied().unwrap_or("icmp").to_lowercase();
    let mut sample = WanQualitySample {
        time,
        wan,
        kind: kind.clone(),
        target: fields
            .get("target")
            .or_else(|| fields.get("interface"))
            .copied()
            .unwrap_or_default()
            .to_string(),
        status: fields.get("status").map(|value| value.to_lowercase()),
        sent: None,
        received: None,
        loss_percent: None,
        min_rtt_ms: None,
        avg_rtt_ms: None,
        max_rtt_ms: None,
        jitter_ms: None,
        stdev_rtt_ms: None,
        tcp_connect_ms: None,
        dns_server: fields.get("server").map(|value| value.to_string()),
        dns_answer: fields.get("answer").map(|value| value.to_string()),
        interface: fields.get("interface").map(|value| value.to_string()),
        running: fields.get("running").and_then(|value| value.parse().ok()),
        tx_queue_drop: fields.get("tx_queue_drop").and_then(|value| value.parse().ok()),
        link_downs: fields.get("link_downs").and_then(|value| value.parse().ok()),
        rx_fcs_error: fields.get("rx_fcs_error").and_then(|value| value.parse().ok()),
        rx_align_error: fields.get("rx_align_error").and_then(|value| value.parse().ok()),
        tx_collision: fields.get("tx_collision").and_then(|value| value.parse().ok()),
        active: None,
        hard_bad_cycles: None,
        severe_bad_cycles: None,
        quality_bad_cycles: None,
        last_switch_uptime_ms: None,
    };

    match kind.as_str() {
        "icmp" => {
            let sent = fields.get("sent")?.parse::<u32>().ok()?;
            let received = fields.get("received")?.parse::<u32>().ok()?;
            sample.sent = Some(sent);
            sample.received = Some(received);
            sample.loss_percent = (sent > 0).then(|| {
                f64::from(sent.saturating_sub(received)) * 100.0 / f64::from(sent)
            });
            if received > 0 {
                sample.min_rtt_ms = fields.get("min").and_then(|value| routeros_duration_ms(value));
                sample.avg_rtt_ms = fields.get("avg").and_then(|value| routeros_duration_ms(value));
                sample.max_rtt_ms = fields.get("max").and_then(|value| routeros_duration_ms(value));
                sample.jitter_ms = fields
                    .get("jitter")
                    .and_then(|value| routeros_duration_ms(value));
                sample.stdev_rtt_ms = fields
                    .get("stdev")
                    .and_then(|value| routeros_duration_ms(value));
            }
        }
        "tcp" => {
            sample.tcp_connect_ms = Some(routeros_duration_ms(fields.get("connect")?)?);
        }
        "dns" => {}
        "interface" => {
            sample.interface.as_ref()?;
            sample.running?;
            sample.tx_queue_drop?;
            sample.link_downs?;
        }
        _ => return None,
    }
    Some(sample)
}

fn wan_decision_samples(time: String, message: &str) -> Option<Vec<WanQualitySample>> {
    let fields: std::collections::HashMap<&str, &str> = message
        .strip_prefix("WANQUALITY type=decision ")?
        .split_whitespace()
        .filter_map(|field| field.split_once('='))
        .collect();
    let active_wan = fields.get("active")?.to_lowercase();
    if !matches!(active_wan.as_str(), "lmt" | "bite") {
        return None;
    }
    let hard_bad_cycles = fields.get("hard_bad").and_then(|value| value.parse().ok());
    let severe_bad_cycles = fields
        .get("severe_bad")
        .and_then(|value| value.parse().ok());
    let quality_bad_cycles = fields.get("quality_bad").and_then(|value| value.parse().ok());
    let last_switch_uptime_ms = fields
        .get("last_switch")
        .and_then(|value| routeros_duration_ms(value));

    Some(
        ["lmt", "bite"]
            .into_iter()
            .filter_map(|wan| {
                let sent = fields.get(format!("{wan}_sent").as_str())?.parse::<u32>().ok()?;
                let received = fields
                    .get(format!("{wan}_received").as_str())?
                    .parse::<u32>()
                    .ok()?;
                let avg_rtt_ms = fields
                    .get(format!("{wan}_avg").as_str())
                    .and_then(|value| routeros_duration_ms(value));
                let max_rtt_ms = fields
                    .get(format!("{wan}_max").as_str())
                    .and_then(|value| routeros_duration_ms(value));
                let jitter_ms = fields
                    .get(format!("{wan}_jitter").as_str())
                    .and_then(|value| routeros_duration_ms(value));
                let tcp_status = fields
                    .get(format!("{wan}_tcp_status").as_str())
                    .map(|value| value.to_lowercase());
                let tcp_connect_ms = fields
                    .get(format!("{wan}_tcp").as_str())
                    .and_then(|value| routeros_duration_ms(value));
                Some(WanQualitySample {
                    time: time.clone(),
                    wan: wan.to_string(),
                    kind: "decision".to_string(),
                    target: "1.1.1.1:443".to_string(),
                    status: tcp_status,
                    sent: Some(sent),
                    received: Some(received),
                    loss_percent: (sent > 0).then(|| {
                        f64::from(sent.saturating_sub(received)) * 100.0 / f64::from(sent)
                    }),
                    min_rtt_ms: None,
                    avg_rtt_ms,
                    max_rtt_ms,
                    jitter_ms,
                    stdev_rtt_ms: None,
                    tcp_connect_ms,
                    dns_server: None,
                    dns_answer: None,
                    interface: None,
                    running: None,
                    tx_queue_drop: None,
                    link_downs: None,
                    rx_fcs_error: None,
                    rx_align_error: None,
                    tx_collision: None,
                    active: Some(active_wan == wan),
                    hard_bad_cycles,
                    severe_bad_cycles,
                    quality_bad_cycles,
                    last_switch_uptime_ms,
                })
            })
            .collect(),
    )
}

fn wan_quality_samples(time: String, message: &str) -> Vec<WanQualitySample> {
    wan_decision_samples(time.clone(), message)
        .or_else(|| wan_quality_sample(time, message).map(|sample| vec![sample]))
        .unwrap_or_default()
}

fn normalize_wan_quality_samples(samples: &mut Vec<WanQualitySample>) {
    samples.sort_by(|left, right| {
        (
            &left.time,
            &left.wan,
            &left.kind,
            &left.target,
            &left.dns_server,
        )
            .cmp(&(
                &right.time,
                &right.wan,
                &right.kind,
                &right.target,
                &right.dns_server,
            ))
    });
    samples.dedup_by(|left, right| {
        left.time == right.time
            && left.wan == right.wan
            && left.kind == right.kind
            && left.target == right.target
            && left.dns_server == right.dns_server
    });
}

fn disk_wan_quality_samples(api: &mut ApiRos) -> Result<WanQualityResult, String> {
    let rows = api.talk(&["/file/print"]).map_err(|error| error.to_string())?;
    let mut samples = Vec::new();
    let mut history_file_count = 0;

    for (reply, attrs) in rows {
        if reply != "!re" {
            continue;
        }
        let name = attrs.get("=name").map(String::as_str).unwrap_or_default();
        if !name.starts_with("wan-quality.") || !name.ends_with(".txt") {
            continue;
        }
        history_file_count += 1;
        let size = attrs.get("=size").and_then(|value| value.parse().ok()).unwrap_or(0);
        let contents = router_file_contents(api, name, size)?;
        for line in contents.lines() {
            let Some((logged_at, message)) = line.split_once(" script,info ") else {
                continue;
            };
            let Some(time) = disk_log_time(logged_at) else {
                continue;
            };
            samples.extend(wan_quality_samples(time, message));
        }
    }

    normalize_wan_quality_samples(&mut samples);
    Ok(WanQualityResult {
        samples,
        history_file_count,
    })
}

#[cfg(test)]
mod disk_history_tests {
    use super::{
        disk_log_time, normalize_wan_quality_samples, routeros_duration_ms, switch_event,
        wan_decision_samples, wan_quality_sample,
    };

    #[test]
    fn parses_disk_history_record() {
        let event = switch_event(
            disk_log_time("Aug/17/2026 09:57:13").expect("RouterOS disk timestamp"),
            "DUALWAN state=lmt reason=disk-history-start",
        )
        .expect("DUALWAN event");

        assert_eq!(event.time, "2026-08-17 09:57:13");
        assert_eq!(event.state, "lmt");
        assert_eq!(event.reason, "disk-history-start");
    }

    #[test]
    fn ignores_non_wan_disk_record() {
        assert!(switch_event("2026-08-17 09:57:13".to_string(), "login failure").is_none());
    }

    #[test]
    fn parses_routeros_duration_with_submilliseconds() {
        assert_eq!(routeros_duration_ms("26ms506us"), Some(26.506));
        assert_eq!(routeros_duration_ms("26506"), Some(26.506));
        assert_eq!(routeros_duration_ms("1s25ms"), Some(1_025.0));
        assert_eq!(routeros_duration_ms("00:00:01.025"), Some(1_025.0));
        assert_eq!(routeros_duration_ms("4294967295"), None);
        assert_eq!(routeros_duration_ms("invalid"), None);
    }

    #[test]
    fn parses_disk_wan_quality_record() {
        let sample = wan_quality_sample(
            "2026-08-18 10:11:52".to_string(),
            "WANQUALITY wan=lmt target=1.1.1.1 sent=5 received=5 loss=0 min=21ms88us avg=26ms506us max=37ms739us jitter=16ms651us",
        )
        .expect("WANQUALITY sample");

        assert_eq!(sample.wan, "lmt");
        assert_eq!(sample.target, "1.1.1.1");
        assert_eq!(sample.kind, "icmp");
        assert_eq!(sample.sent, Some(5));
        assert_eq!(sample.received, Some(5));
        assert_eq!(sample.loss_percent, Some(0.0));
        assert_eq!(sample.avg_rtt_ms, Some(26.506));
        assert_eq!(sample.jitter_ms, Some(16.651));
        assert_eq!(sample.stdev_rtt_ms, None);
    }

    #[test]
    fn parses_extended_wan_quality_records() {
        let tcp = wan_quality_sample(
            "2026-08-18 13:00:00".to_string(),
            "WANQUALITY type=tcp wan=bite target=1.1.1.1:443 status=up connect=40ms899us",
        )
        .expect("TCP sample");
        assert_eq!(tcp.tcp_connect_ms, Some(40.899));
        assert_eq!(tcp.status.as_deref(), Some("up"));

        let dns = wan_quality_sample(
            "2026-08-18 13:00:00".to_string(),
            "WANQUALITY type=dns wan=lmt target=cloudflare.com server=1.1.1.1 status=up answer=104.16.132.229",
        )
        .expect("DNS sample");
        assert_eq!(dns.dns_server.as_deref(), Some("1.1.1.1"));
        assert_eq!(dns.dns_answer.as_deref(), Some("104.16.132.229"));

        let interface = wan_quality_sample(
            "2026-08-18 13:00:00".to_string(),
            "WANQUALITY type=interface wan=lmt interface=ether3 running=true tx_queue_drop=2696 link_downs=3 rx_fcs_error=0 rx_align_error=0 tx_collision=0",
        )
        .expect("interface sample");
        assert_eq!(interface.running, Some(true));
        assert_eq!(interface.tx_queue_drop, Some(2696));
        assert_eq!(interface.link_downs, Some(3));
    }

    #[test]
    fn preserves_dns_samples_from_distinct_resolvers() {
        let primary = wan_quality_sample(
            "2026-08-20 15:00:00".to_string(),
            "WANQUALITY type=dns wan=lmt target=cloudflare.com server=8.8.8.8 status=up answer=104.16.132.229",
        )
        .expect("primary DNS sample");
        let secondary = wan_quality_sample(
            "2026-08-20 15:00:00".to_string(),
            "WANQUALITY type=dns wan=lmt target=cloudflare.com server=8.8.4.4 status=up answer=104.16.133.229",
        )
        .expect("secondary DNS sample");
        let duplicate = wan_quality_sample(
            "2026-08-20 15:00:00".to_string(),
            "WANQUALITY type=dns wan=lmt target=cloudflare.com server=8.8.8.8 status=up answer=104.16.132.229",
        )
        .expect("duplicate primary DNS sample");
        let mut samples = vec![primary, secondary, duplicate];

        normalize_wan_quality_samples(&mut samples);

        assert_eq!(samples.len(), 2);
        assert_eq!(samples[0].dns_server.as_deref(), Some("8.8.4.4"));
        assert_eq!(samples[1].dns_server.as_deref(), Some("8.8.8.8"));
    }

    #[test]
    fn normalizes_icmp_loss_and_timeout_sentinel() {
        let partial = wan_quality_sample(
            "2026-08-21 12:01:43".to_string(),
            "WANQUALITY type=icmp wan=lmt target=8.8.8.8 sent=5 received=4 loss=200 min=33721 avg=53895 max=80115 jitter=46394 stdev=17030",
        )
        .expect("partial-loss ICMP sample");
        assert_eq!(partial.loss_percent, Some(20.0));
        assert_eq!(partial.avg_rtt_ms, Some(53.895));

        let timeout = wan_quality_sample(
            "2026-08-21 12:01:43".to_string(),
            "WANQUALITY type=icmp wan=bite target=8.8.8.8 sent=5 received=0 loss=1000 min=4294967295 avg=4294967295 max=4294967295 jitter=4294967295 stdev=4294967295",
        )
        .expect("timeout ICMP sample");
        assert_eq!(timeout.loss_percent, Some(100.0));
        assert_eq!(timeout.avg_rtt_ms, None);
        assert_eq!(timeout.jitter_ms, None);
        assert_eq!(timeout.stdev_rtt_ms, None);
    }

    #[test]
    fn expands_combined_decision_record_for_both_wans() {
        let samples = wan_decision_samples(
            "2026-08-24 10:20:00".to_string(),
            "WANQUALITY type=decision active=bite hard_bad=1 severe_bad=3 quality_bad=2 last_switch=00:05:00 lmt_sent=3 lmt_received=2 lmt_avg=00:00:00.120000 lmt_max=00:00:00.250000 lmt_jitter=00:00:00.180000 lmt_tcp_status=up lmt_tcp=00:00:00.090000 bite_sent=3 bite_received=3 bite_avg=00:00:00.030000 bite_max=00:00:00.040000 bite_jitter=00:00:00.015000 bite_tcp_status=up bite_tcp=00:00:00.035000",
        )
        .expect("combined decision record");

        assert_eq!(samples.len(), 2);
        assert_eq!(samples[0].wan, "lmt");
        assert_eq!(samples[0].active, Some(false));
        assert_eq!(samples[0].loss_percent, Some(100.0 / 3.0));
        assert_eq!(samples[0].avg_rtt_ms, Some(120.0));
        assert_eq!(samples[1].wan, "bite");
        assert_eq!(samples[1].active, Some(true));
        assert_eq!(samples[1].tcp_connect_ms, Some(35.0));
        assert_eq!(samples[1].hard_bad_cycles, Some(1));
        assert_eq!(samples[1].severe_bad_cycles, Some(3));
        assert_eq!(samples[1].quality_bad_cycles, Some(2));
        assert_eq!(samples[1].last_switch_uptime_ms, Some(300_000.0));

        let legacy_samples = wan_decision_samples(
            "2026-08-24 10:19:00".to_string(),
            "WANQUALITY type=decision active=lmt hard_bad=0 quality_bad=0 last_switch=00:04:00 lmt_sent=3 lmt_received=3 lmt_avg=00:00:00.030000 lmt_max=00:00:00.040000 lmt_jitter=00:00:00.010000 lmt_tcp_status=up lmt_tcp=00:00:00.035000 bite_sent=3 bite_received=3 bite_avg=00:00:00.032000 bite_max=00:00:00.045000 bite_jitter=00:00:00.012000 bite_tcp_status=up bite_tcp=00:00:00.038000",
        )
        .expect("legacy decision record");
        assert_eq!(legacy_samples[0].severe_bad_cycles, None);
    }
}

/// Читає постійну історію якості обох WAN із `wan-quality.*.txt` на диску RouterOS.
pub fn read_wan_quality_impl() -> Result<String, String> {
    let mut api = connect_and_login(Duration::from_secs(5))?;
    let result = disk_wan_quality_samples(&mut api)?;
    serde_json::to_string(&result).map_err(|error| error.to_string())
}

#[tauri::command]
fn read_wan_quality() -> Result<String, String> {
    read_wan_quality_impl()
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

    let (switch_events, history_file_count) = disk_switch_events(&mut api)?;

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
        history_source: "disk".to_string(),
        history_file_count,
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
            read_wan_quality,
            read_router_log,
            read_router_diagnostic
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
