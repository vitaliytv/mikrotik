mod routeros;

use routeros::{connect_and_login, load_config, read_traffic, ApiRos, PROBE_LMT_PUBLIC, PROBE_ZTE};
use serde::Serialize;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

const MONITOR_INTERVAL: Duration = Duration::from_secs(15);

const STICKY_FAILOVER_SOURCE: &str = r#":global dwState
:global dwActiveBad
:global dwLastDecision

:if ([:typeof $dwState] = "nothing") do={ :set dwState "lmt" }
:if ([:typeof $dwActiveBad] = "nothing") do={ :set dwActiveBad 0 }

:local activeTable "to_WAN1"
:if ($dwState = "bite") do={ :set activeTable "to_WAN2" }

# Only the currently active WAN is probed. There is deliberately no automatic
# failback: a recovered standby is used only after the active WAN fails.
:local edgeReceived [/ping address=212.93.105.242 routing-table=$activeTable count=3 interval=200ms]
:local publicReceived [/ping address=1.1.1.1 routing-table=$activeTable count=3 interval=200ms]
:local activeGood (($edgeReceived >= 2) || ($publicReceived >= 2))
:local next $dwState
:local reason "active-healthy"

:if ($activeGood) do={
  :set dwActiveBad 0
} else={
  :set dwActiveBad ($dwActiveBad + 1)
  :set reason "active-probes-failed-keep-primary"
  :if ($dwActiveBad >= 3) do={
    :if ($dwState = "lmt") do={ :set next "bite" } else={ :set next "lmt" }
    :set dwActiveBad 0
    :set reason "active-probes-failed-3x-switch-next"
  }
}

:if ($next != $dwState) do={
  :if ($next = "bite") do={
    /ip dhcp-client set [find name="client1"] default-route-tables="main:1,to_WAN1:1,to_WAN2:1"
    :delay 1s
    /ip dhcp-client set [find name="client2"] default-route-tables="main:2,to_WAN1:2,to_WAN2:2"
  } else={
    /ip dhcp-client set [find name="client2"] default-route-tables="main:1,to_WAN1:1,to_WAN2:2"
    :delay 1s
    /ip dhcp-client set [find name="client1"] default-route-tables="main:2,to_WAN1:2,to_WAN2:1"
  }
  :set dwState $next
}

:if ($dwLastDecision != $dwState) do={
  :log warning ("DUALWAN state=" . $dwState . " reason=" . $reason . " edge-received=" . $edgeReceived . "/3 public-received=" . $publicReceived . "/3")
  :set dwLastDecision $dwState
}"#;

fn find_router_item_id(rows: &[(String, std::collections::HashMap<String, String>)], name: &str) -> Option<String> {
    rows.iter().find_map(|(reply, attrs)| {
        (reply == "!re" && attrs.get("=name").map(String::as_str) == Some(name))
            .then(|| attrs.get("=.id").cloned())
            .flatten()
    })
}

fn set_failover_state(api: &mut ApiRos, state: &str) -> Result<(), String> {
    let clients = api.talk(&["/ip/dhcp-client/print"]).map_err(|e| e.to_string())?;
    let bite_id = find_router_item_id(&clients, "client1").ok_or_else(|| "BITE DHCP client не знайдено".to_string())?;
    let lmt_id = find_router_item_id(&clients, "client2").ok_or_else(|| "LMT DHCP client не знайдено".to_string())?;

    if state == "bite" {
        api.talk(&["/ip/dhcp-client/set", &format!("=.id={bite_id}"), "=default-route-tables=main:1,to_WAN1:1,to_WAN2:1"]).map_err(|e| e.to_string())?;
        api.talk(&["/ip/dhcp-client/set", &format!("=.id={lmt_id}"), "=default-route-tables=main:2,to_WAN1:2,to_WAN2:2"]).map_err(|e| e.to_string())?;
    } else {
        api.talk(&["/ip/dhcp-client/set", &format!("=.id={lmt_id}"), "=default-route-tables=main:1,to_WAN1:1,to_WAN2:2"]).map_err(|e| e.to_string())?;
        api.talk(&["/ip/dhcp-client/set", &format!("=.id={bite_id}"), "=default-route-tables=main:2,to_WAN1:2,to_WAN2:1"]).map_err(|e| e.to_string())?;
    }

    let temporary_name = "DUALWAN-set-state";
    let source = format!(":global dwState; :global dwActiveBad; :global dwLmtBad; :global dwLmtGood; :global dwLastDecision; :set dwState \"{state}\"; :set dwActiveBad 0; :set dwLmtBad 0; :set dwLmtGood 0; :set dwLastDecision \"{state}\"");
    let _ = api.talk(&["/system/script/remove", &format!("=numbers={temporary_name}")]);
    api.talk(&["/system/script/add", &format!("=name={temporary_name}"), &format!("=source={source}")]).map_err(|e| e.to_string())?;
    let run = api.talk(&["/system/script/run", &format!("=number={temporary_name}")]).map_err(|e| e.to_string())?;
    let _ = api.talk(&["/system/script/remove", &format!("=numbers={temporary_name}")]);
    if let Some((_, attrs)) = run.iter().find(|(reply, _)| reply == "!trap") {
        return Err(attrs.get("=message").cloned().unwrap_or_else(|| "Не вдалося оновити state".to_string()));
    }
    Ok(())
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
    lmt_good_cycles: String,
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
                lmt_good_cycles: String::new(),
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
                lmt_good_cycles: String::new(),
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
        controller_state: globals.get("dwState").cloned().unwrap_or_else(|| "unknown".to_string()),
        lmt_bad_cycles: globals.get("dwActiveBad").or_else(|| globals.get("dwLmtBad")).cloned().unwrap_or_default(),
        lmt_good_cycles: globals.get("dwLmtGood").cloned().unwrap_or_default(),
        last_decision: globals.get("dwLastDecision").cloned().unwrap_or_default(),
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

/// Repairs the RouterOS `as-value` ping accounting bug in the current
/// DUALWAN-health source. The UI requires an explicit confirmation before it
/// invokes this write operation.
#[tauri::command]
fn repair_failover_ping() -> Result<String, String> {
    let mut api = connect_and_login(Duration::from_secs(15))?;
    let script = api
        .talk(&["/system/script/print"])
        .map_err(|e| e.to_string())?
        .into_iter()
        .find_map(|(reply, attrs)| {
            (reply == "!re" && attrs.get("=name").map(String::as_str) == Some("DUALWAN-health"))
                .then_some(attrs)
        })
        .ok_or_else(|| "DUALWAN-health не знайдено".to_string())?;
    let id = script.get("=.id").cloned().ok_or_else(|| "ID DUALWAN-health відсутній".to_string())?;
    let source = script.get("=source").cloned().ok_or_else(|| "source DUALWAN-health відсутній".to_string())?;
    let old_edge = ":foreach reply in=[/ping address=212.93.105.242 count=3 interval=200ms as-value] do={ :set edgeReceived ($edgeReceived + 1) }";
    let old_public = ":foreach reply in=[/ping address=1.1.1.1 count=3 interval=200ms as-value] do={ :set publicReceived ($publicReceived + 1) }";
    let new_edge = ":foreach reply in=[/ping address=212.93.105.242 count=3 interval=200ms as-value] do={\n  :if (($reply->\"status\") != \"timeout\") do={ :set edgeReceived ($edgeReceived + 1) }\n}";
    let new_public = ":foreach reply in=[/ping address=1.1.1.1 count=3 interval=200ms as-value] do={\n  :if (($reply->\"status\") != \"timeout\") do={ :set publicReceived ($publicReceived + 1) }\n}";
    if source.contains(new_edge) && source.contains(new_public) {
        return Ok("Ping accounting already repaired".to_string());
    }
    let updated = source.replace(old_edge, new_edge).replace(old_public, new_public);
    if updated == source {
        return Err("Невідома версія DUALWAN-health: автоматичне виправлення скасовано".to_string());
    }
    let result = api
        .talk(&["/system/script/set", &format!("=.id={id}"), &format!("=source={updated}")])
        .map_err(|e| e.to_string())?;
    if let Some((_, attrs)) = result.iter().find(|(reply, _)| reply == "!trap") {
        return Err(attrs.get("=message").cloned().unwrap_or_else(|| "RouterOS відхилив source".to_string()));
    }
    Ok("Ping accounting repaired; scheduler will re-evaluate LMT within 15 seconds".to_string())
}

#[tauri::command]
fn hold_bite_primary() -> Result<String, String> {
    let mut api = connect_and_login(Duration::from_secs(15))?;
    let scheduler_id = api.talk(&["/system/scheduler/print"]).map_err(|e| e.to_string())?.into_iter()
        .find_map(|(reply, attrs)| (reply == "!re" && attrs.get("=name").map(String::as_str) == Some("DUALWAN-health-every-5s")).then(|| attrs.get("=.id").cloned()).flatten())
        .ok_or_else(|| "DUALWAN scheduler не знайдено".to_string())?;

    api.talk(&["/system/scheduler/set", &format!("=.id={scheduler_id}"), "=disabled=yes"]).map_err(|e| e.to_string())?;
    set_failover_state(&mut api, "bite")?;
    Ok("BITE is primary; DUALWAN scheduler paused to prevent LMT flapping".to_string())
}

/// Reverses `hold_bite_primary`: restores the original per-table route
/// priorities (LMT primary for main/to_WAN1, BITE primary for to_WAN2 — not
/// a plain mirror of the hold, `to_WAN2` prefers BITE by design even when LMT
/// is primary overall) and re-enables the DUALWAN-health scheduler so the
/// automatic failover resumes evaluating on its own.
#[tauri::command]
fn resume_auto_failover() -> Result<String, String> {
    let mut api = connect_and_login(Duration::from_secs(15))?;
    let scheduler_id = find_router_item_id(&api.talk(&["/system/scheduler/print"]).map_err(|e| e.to_string())?, "DUALWAN-health-every-5s")
        .ok_or_else(|| "DUALWAN scheduler не знайдено".to_string())?;

    let script_id = find_router_item_id(&api.talk(&["/system/script/print"]).map_err(|e| e.to_string())?, "DUALWAN-health")
        .ok_or_else(|| "DUALWAN-health не знайдено".to_string())?;
    api.talk(&["/system/script/set", &format!("=.id={script_id}"), &format!("=source={STICKY_FAILOVER_SOURCE}")]).map_err(|e| e.to_string())?;
    set_failover_state(&mut api, "lmt")?;

    api.talk(&["/system/scheduler/set", &format!("=.id={scheduler_id}"), "=disabled=no"]).map_err(|e| e.to_string())?;

    Ok("Sticky auto-failover enabled: LMT is primary, only the active WAN is checked, and there is no automatic failback".to_string())
}

#[tauri::command]
fn force_next_wan() -> Result<String, String> {
    let mut api = connect_and_login(Duration::from_secs(15))?;
    let globals: std::collections::HashMap<String, String> = api
        .talk(&["/system/script/environment/print"])
        .map_err(|e| e.to_string())?
        .into_iter()
        .filter_map(|(reply, attrs)| (reply == "!re").then(|| Some((attrs.get("=name")?.clone(), attrs.get("=value").cloned().unwrap_or_default())))?)
        .collect();
    let current = globals.get("dwState").map(String::as_str).unwrap_or("lmt");
    let next = if current == "bite" { "lmt" } else { "bite" };
    set_failover_state(&mut api, next)?;
    Ok(format!("Forced switch completed: {} is now primary", next.to_uppercase()))
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
            if let Some(window) = app.get_webview_window("main") {
                let base_title = window.title().unwrap_or_default();
                let version = &app.package_info().version;
                let _ = window.set_title(&format!("{base_title} v{version}"));
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![read_wan_speed, read_router_log, read_router_diagnostic, repair_failover_ping, hold_bite_primary, resume_auto_failover, force_next_wan])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
