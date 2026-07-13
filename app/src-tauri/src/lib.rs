mod routeros;

use regex::Regex;
use routeros::{
    channel_for_host, connect_and_login, get_gateways, ping_via_gw, set_wan_routes, PROBE_SOYEA, PROBE_ZTE,
};
use serde::Serialize;
use std::sync::OnceLock;
use std::time::Duration;
use tauri::{AppHandle, Emitter};

const MONITOR_INTERVAL: Duration = Duration::from_secs(15);
const MONITOR_PING_COUNT: u32 = 3;
const MEASURE_NOW_PING_COUNT: u32 = 5;

#[derive(Serialize, Clone)]
struct WanSample {
    ts: String,
    zte_avg: Option<f64>,
    zte_loss: f64,
    soyea_avg: Option<f64>,
    soyea_loss: f64,
}

fn now_iso() -> String {
    chrono::Local::now().format("%Y-%m-%dT%H:%M:%S").to_string()
}

fn measure_sample(ping_count: u32) -> WanSample {
    let ts = now_iso();
    match connect_and_login(Duration::from_secs(8)) {
        Ok(mut api) => {
            let (gw1, gw2) = get_gateways(&mut api);
            let (zte_avg, _, zte_loss) = match gw1 {
                Some(gw) => ping_via_gw(&mut api, PROBE_ZTE, &gw, ping_count),
                None => (None, None, 100.0),
            };
            let (soyea_avg, _, soyea_loss) = match gw2 {
                Some(gw) => ping_via_gw(&mut api, PROBE_SOYEA, &gw, ping_count),
                None => (None, None, 100.0),
            };
            WanSample { ts, zte_avg, zte_loss, soyea_avg, soyea_loss }
        }
        Err(_) => WanSample { ts, zte_avg: None, zte_loss: 100.0, soyea_avg: None, soyea_loss: 100.0 },
    }
}

fn start_monitor_thread(app: AppHandle) {
    std::thread::spawn(move || loop {
        let start = std::time::Instant::now();
        let sample = measure_sample(MONITOR_PING_COUNT);
        let _ = app.emit("wan-sample", &sample);
        let elapsed = start.elapsed();
        if elapsed < MONITOR_INTERVAL {
            std::thread::sleep(MONITOR_INTERVAL - elapsed);
        }
    });
}

#[tauri::command]
fn measure_now() -> String {
    let s = measure_sample(MEASURE_NOW_PING_COUNT);
    format!(
        "{}  LMT={} BITE={}",
        s.ts,
        s.zte_avg.map(|v| format!("{}мс/{}%", v, s.zte_loss)).unwrap_or_else(|| "недоступний".into()),
        s.soyea_avg.map(|v| format!("{}мс/{}%", v, s.soyea_loss)).unwrap_or_else(|| "недоступний".into()),
    )
}

#[tauri::command]
fn read_wan_speed() -> Result<String, String> {
    let mut api = connect_and_login(Duration::from_secs(10))?;
    let rows = api
        .talk(&["/interface/monitor-traffic", "=interface=ether1,ether3", "=once="])
        .map_err(|e| e.to_string())?;
    let mut zte = None;
    let mut soyea = None;
    for (r, attrs) in rows {
        if r != "!re" {
            continue;
        }
        let rx = attrs.get("=rx-bits-per-second").and_then(|v| v.parse::<i64>().ok()).unwrap_or(0);
        let tx = attrs.get("=tx-bits-per-second").and_then(|v| v.parse::<i64>().ok()).unwrap_or(0);
        match attrs.get("=name").map(|s| s.as_str()) {
            Some("ether3") => zte = Some((rx, tx)),
            Some("ether1") => soyea = Some((rx, tx)),
            _ => {}
        }
    }
    let mut obj = serde_json::json!({ "ts": now_iso() });
    if let Some((rx, tx)) = zte {
        obj["zte"] = serde_json::json!({ "rx_bps": rx, "tx_bps": tx });
    }
    if let Some((rx, tx)) = soyea {
        obj["soyea"] = serde_json::json!({ "rx_bps": rx, "tx_bps": tx });
    }
    Ok(obj.to_string())
}

#[tauri::command]
fn toggle_wan(channel: String, on: bool) -> Result<String, String> {
    let prefix = match channel.as_str() {
        "zte" => "1",
        "soyea" => "2",
        _ => return Err(format!("невідомий канал: {}", channel)),
    };
    let mut api = connect_and_login(Duration::from_secs(15))?;
    let n = set_wan_routes(&mut api, prefix, on);
    let action = if on { "увімкнено" } else { "вимкнено вручну" };
    Ok(format!("{}: {} ({} маршрутів)", channel, action, n))
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

fn flap_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r#"^route .* changed by netwatch:type: icmp, host: ([\d.]+)/action:(\d+) \(.*disabled=(yes|no)"#).unwrap()
    })
}

#[tauri::command]
fn read_router_log() -> Result<String, String> {
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
            if comment.starts_with("LB-w") && attrs.get("=dst-address").map(|s| s.as_str()) == Some("0.0.0.0/0") {
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
            let host = caps.get(1).unwrap().as_str().to_string();
            let action_id = caps.get(2).unwrap().as_str().to_string();
            let disabled = caps.get(3).unwrap().as_str();
            let key = (t.clone(), action_id);
            if !seen.insert(key) {
                continue;
            }
            flap_events.push(FlapEvent {
                time: t,
                channel: channel_for_host(&host).to_string(),
                host,
                action: if disabled == "yes" { "down".to_string() } else { "up".to_string() },
            });
        } else if keywords.iter().any(|k| msg.to_lowercase().contains(k)) {
            other_events.push(OtherEvent { time: t, message: msg });
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
    let raw_log: Vec<RawLogLine> = raw_log.into_iter().skip(raw_log_len.saturating_sub(300)).collect();
    let other_len = other_events.len();
    let other_events: Vec<OtherEvent> = other_events.into_iter().skip(other_len.saturating_sub(100)).collect();

    let result = RouterLogResult { netwatch, routes, flap_events, other_events, raw_log, log_total_lines };
    serde_json::to_string(&result).map_err(|e| e.to_string())
}

// ---------- відновлення конфігурації failover (recovery) ----------

fn add_route(api: &mut routeros::ApiRos, comment: &str, gw: &str, dist: u32, table: Option<&str>, dst: &str, extra: &[&str]) {
    let mut w = vec![
        "/ip/route/add".to_string(),
        format!("=dst-address={}", dst),
        format!("=gateway={}", gw),
        format!("=distance={}", dist),
        format!("=comment={}", comment),
    ];
    if dst == "0.0.0.0/0" {
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

fn add_netwatch(api: &mut routeros::ApiRos, host: &str, comment: &str, up_script: &str, down_script: &str) -> bool {
    let host_arg = format!("=host={}", host);
    let comment_arg = format!("=comment={}", comment);
    let up_arg = format!("=up-script={}", up_script);
    let dn_arg = format!("=down-script={}", down_script);
    let mut words: Vec<&str> = vec!["/tool/netwatch/add", &host_arg, &comment_arg, &up_arg, &dn_arg];
    words.extend_from_slice(&["=type=icmp", "=interval=25s", "=timeout=2s", "=packet-count=12", "=thr-loss-percent=55"]);
    match api.talk(&words) {
        Ok(rows) => !rows.iter().any(|(r, _)| r == "!trap"),
        Err(_) => false,
    }
}

fn test_wan_alive(api: &mut routeros::ApiRos, gwip: &str) -> u32 {
    let _ = api.talk(&["/ip/route/add", "=dst-address=9.9.9.9/32", &format!("=gateway={}", gwip), "=comment=TESTX"]);
    let rows = api.talk(&["/ping", "=address=9.9.9.9", "=count=3"]).unwrap_or_default();
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

#[tauri::command]
fn restore_failover_config() -> Result<String, String> {
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
            if attrs.get("=comment").map(|c| c.starts_with("LB")).unwrap_or(false) {
                if let Some(id) = attrs.get("=.id") {
                    let _ = api.talk(&["/tool/netwatch/remove", &format!("=.id={}", id)]);
                }
            }
        }
    }

    if let Some(gw) = gw1.clone() {
        add_route(&mut api, "LB-nw1", &gw, 1, None, &format!("{}/32", PROBE_ZTE), &["=scope=10"]);
        add_route(&mut api, "LB-w1m", &gw, 1, None, "0.0.0.0/0", &[]);
        add_route(&mut api, "LB-w1t1", &gw, 1, Some("to_WAN1"), "0.0.0.0/0", &[]);
        add_route(&mut api, "LB-w1t2b", &gw, 2, Some("to_WAN2"), "0.0.0.0/0", &[]);
    }
    if let Some(gw) = gw2.clone() {
        add_route(&mut api, "LB-nw2", &gw, 1, None, &format!("{}/32", PROBE_SOYEA), &["=scope=10"]);
        add_route(&mut api, "LB-w2m", &gw, 2, None, "0.0.0.0/0", &[]);
        add_route(&mut api, "LB-w2t2", &gw, 1, Some("to_WAN2"), "0.0.0.0/0", &[]);
        add_route(&mut api, "LB-w2t1b", &gw, 2, Some("to_WAN1"), "0.0.0.0/0", &[]);
    }

    let up1 = r#"/ip route enable [find comment~"^LB-w1"]"#;
    let dn1 = r#":if ([:len [/ip route find comment~"^LB-w2" disabled=no]] > 0) do={ /ip route disable [find comment~"^LB-w1"] } else={ :log warning "LBnw1 guard: WAN2 already disabled; keeping WAN1 enabled" }"#;
    let up2 = r#"/ip route enable [find comment~"^LB-w2"]"#;
    let dn2 = r#":if ([:len [/ip route find comment~"^LB-w1" disabled=no]] > 0) do={ /ip route disable [find comment~"^LB-w2"] } else={ :log warning "LBnw2 guard: WAN1 already disabled; keeping WAN2 enabled" }"#;

    let mut nw_ok = true;
    if gw1.is_some() {
        nw_ok = add_netwatch(&mut api, PROBE_ZTE, "LBnw1", up1, dn1);
        if !nw_ok {
            log!("  netwatch WAN1 TRAP (можливо device-mode)");
        }
    }
    if gw2.is_some() && nw_ok {
        nw_ok = add_netwatch(&mut api, PROBE_SOYEA, "LBnw2", up2, dn2);
        if !nw_ok {
            log!("  netwatch WAN2 TRAP (можливо device-mode)");
        }
    }

    if nw_ok {
        log!("✅ netwatch авто-failover увімкнено. Чекаю 14с на першу перевірку...");
        std::thread::sleep(Duration::from_secs(14));
    } else {
        log!("⚠️ netwatch недоступний (device-mode). Роблю РУЧНИЙ failover: вимкну мертвий канал.");
        if let Some(gw) = &gw1 {
            if test_wan_alive(&mut api, gw) == 0 {
                set_wan_routes(&mut api, "1", false);
                log!("  WAN1 (LMT) мертвий -> вимкнено, все через WAN2");
            }
        }
        if let Some(gw) = &gw2 {
            if test_wan_alive(&mut api, gw) == 0 {
                set_wan_routes(&mut api, "2", false);
                log!("  WAN2 (BITE) мертвий -> вимкнено, все через WAN1");
            }
        }
    }

    log!("\n-- netwatch стан --");
    if let Ok(rows) = api.talk(&["/tool/netwatch/print"]) {
        for (r, attrs) in rows {
            if r == "!re" && attrs.get("=comment").map(|c| c.starts_with("LBnw")).unwrap_or(false) {
                log!("   {}: {}", attrs.get("=host").cloned().unwrap_or_default(), attrs.get("=status").cloned().unwrap_or_default());
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
            if c.starts_with("LB-w") && attrs.get("=dst-address").map(|s| s.as_str()) == Some("0.0.0.0/0") {
                log!(
                    "   {:9} table={:8} active={} disabled={}",
                    c,
                    attrs.get("=routing-table").cloned().unwrap_or_else(|| "main".to_string()),
                    attrs.get("=active").cloned().unwrap_or_default(),
                    attrs.get("=disabled").cloned().unwrap_or_default()
                );
            }
        }
    }

    log!("\n✅ Готово.");
    Ok(out)
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
        .invoke_handler(tauri::generate_handler![
            measure_now,
            read_wan_speed,
            read_router_log,
            toggle_wan,
            restore_failover_config
        ]);

    #[cfg(debug_assertions)]
    let builder = builder.plugin(tauri_plugin_mcp_bridge::init());

    builder.run(tauri::generate_context!()).expect("error while running tauri application");
}
