use std::process::Command;

fn home_path(name: &str) -> std::path::PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/Users/vitalii".to_string());
    std::path::Path::new(&home).join(name)
}

#[tauri::command]
fn read_wan_csv() -> Result<String, String> {
    std::fs::read_to_string(home_path("wan_log.csv")).map_err(|e| e.to_string())
}

#[tauri::command]
fn read_wan_state() -> Result<String, String> {
    std::fs::read_to_string(home_path("wan_state.json")).map_err(|e| e.to_string())
}

#[tauri::command]
fn run_wan_monitor() -> Result<String, String> {
    let output = Command::new("/usr/bin/python3")
        .arg(home_path("wan_monitor.py"))
        .output()
        .map_err(|e| e.to_string())?;
    let mut text = String::from_utf8_lossy(&output.stdout).to_string();
    text.push_str(&String::from_utf8_lossy(&output.stderr));
    Ok(text)
}

#[tauri::command]
fn read_wan_speed() -> Result<String, String> {
    let output = Command::new("/usr/bin/python3")
        .arg(home_path("wan_speed.py"))
        .output()
        .map_err(|e| e.to_string())?;
    if !output.stdout.is_empty() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).to_string())
    }
}

#[tauri::command]
fn read_router_log() -> Result<String, String> {
    let output = Command::new("/usr/bin/python3")
        .arg(home_path("wan_router_log.py"))
        .output()
        .map_err(|e| e.to_string())?;
    if !output.stdout.is_empty() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).to_string())
    }
}

#[tauri::command]
fn toggle_wan(channel: String, on: bool) -> Result<String, String> {
    let output = Command::new("/usr/bin/python3")
        .arg(home_path("wan_toggle.py"))
        .arg(&channel)
        .arg(if on { "on" } else { "off" })
        .output()
        .map_err(|e| e.to_string())?;
    let mut text = String::from_utf8_lossy(&output.stdout).to_string();
    text.push_str(&String::from_utf8_lossy(&output.stderr));
    if output.status.success() {
        Ok(text)
    } else {
        Err(text)
    }
}

#[tauri::command]
fn restore_failover_config() -> Result<String, String> {
    let output = Command::new("/usr/bin/python3")
        .arg(home_path("fix_mikrotik.py"))
        .output()
        .map_err(|e| e.to_string())?;
    let mut text = String::from_utf8_lossy(&output.stdout).to_string();
    text.push_str(&String::from_utf8_lossy(&output.stderr));
    if output.status.success() {
        Ok(text)
    } else {
        Err(text)
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_agent::init())
        .invoke_handler(tauri::generate_handler![
            read_wan_csv,
            read_wan_state,
            run_wan_monitor,
            read_wan_speed,
            read_router_log,
            toggle_wan,
            restore_failover_config
        ]);

    #[cfg(debug_assertions)]
    let builder = builder.plugin(tauri_plugin_mcp_bridge::init());

    builder
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
