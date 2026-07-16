#[path = "../routeros.rs"]
mod routeros;

use routeros::connect_and_login;
use std::time::Duration;

fn main() -> Result<(), String> {
    let mut api = connect_and_login(Duration::from_secs(20))?;
    println!("[environment]");
    for (reply, attrs) in api.talk(&["/system/script/environment/print"]).map_err(|e| e.to_string())? {
        if reply == "!re" { println!("{}={}", attrs.get("=name").cloned().unwrap_or_default(), attrs.get("=value").cloned().unwrap_or_default()); }
    }
    println!("[script]");
    for (reply, attrs) in api.talk(&["/system/script/print"]).map_err(|e| e.to_string())? {
        if reply == "!re" && attrs.get("=name").map(String::as_str) == Some("DUALWAN-health") {
            println!("invalid={} run-count={}", attrs.get("=invalid").cloned().unwrap_or_default(), attrs.get("=run-count").cloned().unwrap_or_default());
            println!("{}", attrs.get("=source").cloned().unwrap_or_default());
        }
    }
    println!("[script-log]");
    for (reply, attrs) in api.talk(&["/log/print"]).map_err(|e| e.to_string())? {
        if reply == "!re" {
            let msg = attrs.get("=message").cloned().unwrap_or_default();
            if msg.contains("DUALWAN") || msg.contains("script") || msg.contains("ping") { println!("{} {}", attrs.get("=time").cloned().unwrap_or_default(), msg); }
        }
    }
    Ok(())
}
