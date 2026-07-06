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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            read_wan_csv,
            read_wan_state,
            run_wan_monitor
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
