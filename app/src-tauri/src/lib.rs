mod routeros;

use regex::Regex;
use routeros::{channel_for_host, connect_and_login, read_traffic, ApiRos};
use serde::Serialize;
use std::sync::OnceLock;
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

// ---------- лог роутера (netwatch flap-події) ----------

#[derive(Serialize)]
struct NetwatchInfo {
    comment: String,
    host: String,
    channel: String,
    status: String,
    since: String,
    interval: String,
    packet_count: String,
    thr_loss_percent: String,
}

#[derive(Serialize)]
struct RouteInfo {
    comment: String,
    active: String,
    disabled: String,
}

#[derive(Serialize)]
struct FlapEvent {
    time: String,
    channel: String,
    host: String,
    action: String,
}

#[derive(Serialize)]
struct OtherEvent {
    time: String,
    message: String,
}

#[derive(Serialize)]
struct RawLogLine {
    time: String,
    topics: String,
    message: String,
}

#[derive(Serialize)]
struct RouterLogResult {
    netwatch: Vec<NetwatchInfo>,
    routes: Vec<RouteInfo>,
    flap_events: Vec<FlapEvent>,
    other_events: Vec<OtherEvent>,
    raw_log: Vec<RawLogLine>,
    log_total_lines: usize,
}

// Matches netwatch's own summary line ("LBnw2 probe down -> lbController"),
// not the verbose "route ... changed by netwatch ..." lines — those also
// carry a `/script:lbController/` segment between host and action that an
// earlier version of this regex didn't account for, silently dropping every
// flap. The probe line is simpler and doesn't depend on script/action-id
// formatting at all.
fn flap_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^(LBnw\d+) probe (up|down)").unwrap())
}

pub fn read_router_log_impl() -> Result<String, String> {
    let mut api = connect_and_login(Duration::from_secs(20))?;

    let mut netwatch = Vec::new();
    if let Ok(rows) = api.talk(&["/tool/netwatch/print"]) {
        for (r, attrs) in rows {
            if r != "!re" {
                continue;
            }
            let comment = attrs.get("=comment").cloned().unwrap_or_default();
            if !comment.starts_with("LBnw") {
                continue;
            }
            let host = attrs.get("=host").cloned().unwrap_or_default();
            netwatch.push(NetwatchInfo {
                channel: channel_for_host(&host).to_string(),
                comment,
                host,
                status: attrs.get("=status").cloned().unwrap_or_default(),
                since: attrs.get("=since").cloned().unwrap_or_default(),
                interval: attrs.get("=interval").cloned().unwrap_or_default(),
                packet_count: attrs.get("=packet-count").cloned().unwrap_or_default(),
                thr_loss_percent: attrs.get("=thr-loss-percent").cloned().unwrap_or_default(),
            });
        }
    }

    let mut routes = Vec::new();
    if let Ok(rows) = api.talk(&["/ip/route/print"]) {
        for (r, attrs) in rows {
            if r != "!re" {
                continue;
            }
            let comment = attrs.get("=comment").cloned().unwrap_or_default();
            if comment.starts_with("LB-w")
                && attrs.get("=dst-address").map(|s| s.as_str()) == Some("0.0.0.0/0")
            {
                routes.push(RouteInfo {
                    comment,
                    active: attrs.get("=active").cloned().unwrap_or_default(),
                    disabled: attrs.get("=disabled").cloned().unwrap_or_default(),
                });
            }
        }
    }

    let log_rows = api.talk(&["/log/print"]).map_err(|e| e.to_string())?;
    let log_rows: Vec<_> = log_rows.into_iter().filter(|(r, _)| r == "!re").collect();

    let mut flap_events = Vec::new();
    let mut other_events = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let keywords = ["dhcp", "ether1", "ether3", "link", "ppp"];
    for (_, attrs) in &log_rows {
        let msg = attrs.get("=message").cloned().unwrap_or_default();
        let t = attrs.get("=time").cloned().unwrap_or_default();
        if let Some(caps) = flap_re().captures(&msg) {
            let comment = caps.get(1).unwrap().as_str().to_string(); // "LBnw1" / "LBnw2"
            let direction = caps.get(2).unwrap().as_str().to_string();
            let key = (t.clone(), comment.clone(), direction.clone());
            if !seen.insert(key) {
                continue;
            }
            let channel = match comment.as_str() {
                "LBnw1" => "zte",
                "LBnw2" => "soyea",
                _ => "?",
            };
            flap_events.push(FlapEvent {
                time: t,
                channel: channel.to_string(),
                host: comment,
                action: if direction == "down" {
                    "down".to_string()
                } else {
                    "up".to_string()
                },
            });
        } else if keywords.iter().any(|k| msg.to_lowercase().contains(k)) {
            other_events.push(OtherEvent {
                time: t,
                message: msg,
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
    let other_len = other_events.len();
    let other_events: Vec<OtherEvent> = other_events
        .into_iter()
        .skip(other_len.saturating_sub(100))
        .collect();

    let result = RouterLogResult {
        netwatch,
        routes,
        flap_events,
        other_events,
        raw_log,
        log_total_lines,
    };
    serde_json::to_string(&result).map_err(|e| e.to_string())
}

#[tauri::command]
fn read_router_log() -> Result<String, String> {
    read_router_log_impl()
}

// Legacy write operations are deliberately excluded from the read-only viewer.
// RouterOS owns the failover configuration and decisions.
#[cfg(any())]
mod legacy_write_operations {
    use super::*;

    // ---------- відновлення конфігурації failover (recovery) ----------

    fn add_route(
        api: &mut routeros::ApiRos,
        comment: &str,
        gw: &str,
        dist: u32,
        table: Option<&str>,
        dst: &str,
        extra: &[&str],
    ) {
        let mut w = vec![
            "/ip/route/add".to_string(),
            format!("=dst-address={}", dst),
            format!("=gateway={}", gw),
            format!("=distance={}", dist),
            format!("=comment={}", comment),
        ];
        // BITE is a blind failover path: never send health-check pings to it.
        if dst == "0.0.0.0/0" && !comment.starts_with("LB-w2") {
            w.push("=check-gateway=ping".to_string());
        }
        if let Some(t) = table {
            w.push(format!("=routing-table={}", t));
        }
        for e in extra {
            w.push(e.to_string());
        }
        let words: Vec<&str> = w.iter().map(|s| s.as_str()).collect();
        let _ = api.talk(&words);
    }

    fn add_netwatch(
        api: &mut routeros::ApiRos,
        host: &str,
        comment: &str,
        up_script: &str,
        down_script: &str,
    ) -> bool {
        let host_arg = format!("=host={}", host);
        let comment_arg = format!("=comment={}", comment);
        let up_arg = format!("=up-script={}", up_script);
        let dn_arg = format!("=down-script={}", down_script);
        let mut words: Vec<&str> = vec![
            "/tool/netwatch/add",
            &host_arg,
            &comment_arg,
            &up_arg,
            &dn_arg,
        ];
        words.extend_from_slice(&[
            "=type=icmp",
            "=interval=25s",
            "=timeout=2s",
            "=packet-count=12",
            "=thr-loss-percent=55",
        ]);
        match api.talk(&words) {
            Ok(rows) => !rows.iter().any(|(r, _)| r == "!trap"),
            Err(_) => false,
        }
    }

    fn test_wan_alive(api: &mut routeros::ApiRos, gwip: &str) -> u32 {
        let _ = api.talk(&[
            "/ip/route/add",
            "=dst-address=9.9.9.9/32",
            &format!("=gateway={}", gwip),
            "=comment=TESTX",
        ]);
        let rows = api
            .talk(&["/ping", "=address=9.9.9.9", "=count=3"])
            .unwrap_or_default();
        let rec = rows
            .into_iter()
            .filter(|(r, _)| r == "!re")
            .last()
            .and_then(|(_, a)| a.get("=received").and_then(|v| v.parse::<u32>().ok()))
            .unwrap_or(0);
        if let Ok(rows) = api.talk(&["/ip/route/print"]) {
            for (r, attrs) in rows {
                if r == "!re" && attrs.get("=comment").map(|s| s.as_str()) == Some("TESTX") {
                    if let Some(id) = attrs.get("=.id") {
                        let _ = api.talk(&["/ip/route/remove", &format!("=.id={}", id)]);
                    }
                }
            }
        }
        rec
    }

    pub fn restore_failover_config_impl() -> Result<String, String> {
        let mut out = String::new();
        macro_rules! log {
        ($($arg:tt)*) => {{ out.push_str(&format!($($arg)*)); out.push('\n'); }};
    }

        log!("Підключаюсь до MikroTik (до 90с)...");
        let mut api = None;
        for i in 0..30 {
            match connect_and_login(Duration::from_secs(12)) {
                Ok(a) => {
                    log!("  з'єднано.");
                    api = Some(a);
                    break;
                }
                Err(e) => {
                    log!("  спроба {}: {}; чекаю 3с", i + 1, e);
                    std::thread::sleep(Duration::from_secs(3));
                }
            }
        }
        let mut api = match api {
            Some(a) => a,
            None => {
                log!("НЕ ВДАЛОСЯ підключитись.");
                return Err(out);
            }
        };

        let (mut gw1, mut gw2) = get_gateways(&mut api);
        if gw1.is_none() || gw2.is_none() {
            log!("  renew DHCP, чекаю 8с...");
            if let Ok(rows) = api.talk(&["/ip/dhcp-client/print"]) {
                for (r, attrs) in rows {
                    if r == "!re" {
                        if let Some(id) = attrs.get("=.id") {
                            let _ = api.talk(&["/ip/dhcp-client/renew", &format!("=.id={}", id)]);
                        }
                    }
                }
            }
            std::thread::sleep(Duration::from_secs(8));
            let (a, b) = get_gateways(&mut api);
            gw1 = a;
            gw2 = b;
        }
        log!("WAN1 LMT gw={:?} | WAN2 BITE gw={:?}", gw1, gw2);
        if gw1.is_none() && gw2.is_none() {
            log!("Жоден канал без шлюзу — перевір кабелі.");
            return Err(out);
        }

        // прибрати старе
        if let Ok(rows) = api.talk(&["/ip/route/print"]) {
            for (r, attrs) in rows {
                if r != "!re" {
                    continue;
                }
                let c = attrs.get("=comment").cloned().unwrap_or_default();
                if c.starts_with("LB-") || c.starts_with("TEST") {
                    if let Some(id) = attrs.get("=.id") {
                        let _ = api.talk(&["/ip/route/remove", &format!("=.id={}", id)]);
                    }
                }
            }
        }
        if let Ok(rows) = api.talk(&["/tool/netwatch/print"]) {
            for (r, attrs) in rows {
                if r != "!re" {
                    continue;
                }
                if attrs
                    .get("=comment")
                    .map(|c| c.starts_with("LB"))
                    .unwrap_or(false)
                {
                    if let Some(id) = attrs.get("=.id") {
                        let _ = api.talk(&["/tool/netwatch/remove", &format!("=.id={}", id)]);
                    }
                }
            }
        }

        if let Some(gw) = gw1.clone() {
            add_route(
                &mut api,
                "LB-nw1",
                &gw,
                1,
                None,
                &format!("{}/32", PROBE_ZTE),
                &["=scope=10"],
            );
            add_route(&mut api, "LB-w1m", &gw, 1, None, "0.0.0.0/0", &[]);
            add_route(
                &mut api,
                "LB-w1t1",
                &gw,
                1,
                Some("to_WAN1"),
                "0.0.0.0/0",
                &[],
            );
            add_route(
                &mut api,
                "LB-w1t2b",
                &gw,
                2,
                Some("to_WAN2"),
                "0.0.0.0/0",
                &[],
            );
        }
        if let Some(gw) = gw2.clone() {
            add_route(&mut api, "LB-w2m", &gw, 2, None, "0.0.0.0/0", &[]);
            add_route(
                &mut api,
                "LB-w2t2",
                &gw,
                1,
                Some("to_WAN2"),
                "0.0.0.0/0",
                &[],
            );
            add_route(
                &mut api,
                "LB-w2t1b",
                &gw,
                2,
                Some("to_WAN1"),
                "0.0.0.0/0",
                &[],
            );
        }

        // Only LMT is monitored. BITE is never pinged while LMT is healthy — on
        // LMT down, switch to BITE immediately with no health check of its own
        // ("гірше не буде": we're already on a dead primary, so trying BITE
        // blind can't make things worse). On LMT up, switch straight back.
        // No lbController / quality comparison needed since there's only ever
        // one signal (LMT's own status) driving the decision.
        let up1 = r#"/ip route enable [find comment~"^LB-w1"]; /ip route disable [find comment~"^LB-w2"]"#;
        let dn1 = r#"/ip route disable [find comment~"^LB-w1"]; /ip route enable [find comment~"^LB-w2"]"#;

        let mut nw_ok = true;
        if gw1.is_some() {
            nw_ok = add_netwatch(&mut api, PROBE_ZTE, "LBnw1", up1, dn1);
            if !nw_ok {
                log!("  netwatch WAN1 TRAP (можливо device-mode)");
            }
        }

        if nw_ok {
            log!("✅ netwatch авто-failover увімкнено (лише LMT, BITE — сліпий резерв). Чекаю 14с на першу перевірку...");
            std::thread::sleep(Duration::from_secs(14));
        } else {
            log!("⚠️ netwatch недоступний (device-mode). Роблю РУЧНИЙ failover: вимкну мертвий канал.");
            if let Some(gw) = &gw1 {
                if test_wan_alive(&mut api, gw) == 0 {
                    set_wan_routes(&mut api, "1", false);
                    log!("  WAN1 (LMT) мертвий -> вимкнено, все через WAN2");
                }
            }
        }

        log!("\n-- netwatch стан --");
        if let Ok(rows) = api.talk(&["/tool/netwatch/print"]) {
            for (r, attrs) in rows {
                if r == "!re"
                    && attrs
                        .get("=comment")
                        .map(|c| c.starts_with("LBnw"))
                        .unwrap_or(false)
                {
                    log!(
                        "   {}: {}",
                        attrs.get("=host").cloned().unwrap_or_default(),
                        attrs.get("=status").cloned().unwrap_or_default()
                    );
                }
            }
        }
        log!("-- активні default-маршрути --");
        if let Ok(rows) = api.talk(&["/ip/route/print"]) {
            for (r, attrs) in rows {
                if r != "!re" {
                    continue;
                }
                let c = attrs.get("=comment").cloned().unwrap_or_default();
                if c.starts_with("LB-w")
                    && attrs.get("=dst-address").map(|s| s.as_str()) == Some("0.0.0.0/0")
                {
                    log!(
                        "   {:9} table={:8} active={} disabled={}",
                        c,
                        attrs
                            .get("=routing-table")
                            .cloned()
                            .unwrap_or_else(|| "main".to_string()),
                        attrs.get("=active").cloned().unwrap_or_default(),
                        attrs.get("=disabled").cloned().unwrap_or_default()
                    );
                }
            }
        }

        log!("\n✅ Готово.");
        Ok(out)
    }

    #[tauri::command]
    fn restore_failover_config() -> Result<String, String> {
        restore_failover_config_impl()
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_agent::init())
        .setup(|app| {
            start_monitor_thread(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![read_wan_speed, read_router_log]);

    #[cfg(debug_assertions)]
    let builder = builder.plugin(tauri_plugin_mcp_bridge::init());

    builder
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
