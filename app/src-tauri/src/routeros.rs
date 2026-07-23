// RouterOS API client (binary protocol, MD5 challenge-response login) and
// shared WAN helpers. Native Rust port of the former wan_monitor.py /
// fix_mikrotik.py / wan_router_log.py / wan_speed.py / wan_toggle.py scripts.

use md5::{Digest, Md5};
use std::collections::HashMap;
use std::io::{self, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

pub struct ApiRos {
    stream: TcpStream,
}

impl ApiRos {
    pub fn connect(host: &str, timeout: Duration) -> io::Result<Self> {
        let addr = (host, 8728u16)
            .to_socket_addrs()?
            .next()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no address resolved"))?;
        let stream = TcpStream::connect_timeout(&addr, timeout)?;
        stream.set_read_timeout(Some(timeout))?;
        stream.set_write_timeout(Some(timeout))?;
        Ok(Self { stream })
    }

    pub fn login(&mut self, user: &str, pass: &str) -> io::Result<bool> {
        let resp = self.talk(&[
            "/login",
            &format!("=name={}", user),
            &format!("=password={}", pass),
        ])?;
        for (reply, attrs) in &resp {
            if reply == "!trap" {
                return Ok(false);
            }
            if let Some(ret) = attrs.get("=ret") {
                let challenge = hex_decode(ret);
                let mut hasher = Md5::new();
                hasher.update([0u8]);
                hasher.update(pass.as_bytes());
                hasher.update(&challenge);
                let digest = hasher.finalize();
                self.talk(&[
                    "/login",
                    &format!("=name={}", user),
                    &format!("=response=00{}", hex_encode(&digest)),
                ])?;
            }
        }
        Ok(true)
    }

    pub fn talk(&mut self, words: &[&str]) -> io::Result<Vec<(String, HashMap<String, String>)>> {
        self.write_sentence(words)?;
        let mut results = Vec::new();
        loop {
            let sentence = self.read_sentence()?;
            if sentence.is_empty() {
                continue;
            }
            let reply = sentence[0].clone();
            let mut attrs = HashMap::new();
            for word in &sentence[1..] {
                let key_end = if word.len() > 1 {
                    word[1..].find('=').map(|p| p + 1)
                } else {
                    None
                };
                match key_end {
                    Some(j) => {
                        attrs.insert(word[..j].to_string(), word[j + 1..].to_string());
                    }
                    None => {
                        attrs.insert(word.clone(), String::new());
                    }
                }
            }
            results.push((reply.clone(), attrs));
            if reply == "!done" {
                return Ok(results);
            }
        }
    }

    fn write_sentence(&mut self, words: &[&str]) -> io::Result<()> {
        for w in words {
            self.write_word(w)?;
        }
        self.write_len(0)
    }

    fn read_sentence(&mut self) -> io::Result<Vec<String>> {
        let mut out = Vec::new();
        loop {
            let w = self.read_word()?;
            if w.is_empty() {
                return Ok(out);
            }
            out.push(w);
        }
    }

    fn write_word(&mut self, w: &str) -> io::Result<()> {
        let bytes = w.as_bytes();
        self.write_len(bytes.len())?;
        self.stream.write_all(bytes)
    }

    fn read_word(&mut self) -> io::Result<String> {
        let n = self.read_len()?;
        if n == 0 {
            return Ok(String::new());
        }
        let mut buf = vec![0u8; n];
        self.stream.read_exact(&mut buf)?;
        Ok(String::from_utf8_lossy(&buf).to_string())
    }

    fn write_len(&mut self, l: usize) -> io::Result<()> {
        let l = l as u32;
        let bytes: Vec<u8> = if l < 0x80 {
            vec![l as u8]
        } else if l < 0x4000 {
            let v = l | 0x8000;
            v.to_be_bytes()[2..].to_vec()
        } else if l < 0x20_0000 {
            let v = l | 0xC0_0000;
            v.to_be_bytes()[1..].to_vec()
        } else if l < 0x1000_0000 {
            let v = l | 0xE000_0000;
            v.to_be_bytes().to_vec()
        } else {
            let mut v = vec![0xF0u8];
            v.extend_from_slice(&l.to_be_bytes());
            v
        };
        self.stream.write_all(&bytes)
    }

    fn read_len(&mut self) -> io::Result<usize> {
        let c = self.read_byte()?;
        if c & 0x80 == 0 {
            return Ok(c as usize);
        }
        if c & 0xC0 == 0x80 {
            let b1 = self.read_byte()?;
            return Ok((((c & 0x3F) as usize) << 8) + b1 as usize);
        }
        if c & 0xE0 == 0xC0 {
            let mut n = (c & 0x1F) as usize;
            for _ in 0..2 {
                n = (n << 8) + self.read_byte()? as usize;
            }
            return Ok(n);
        }
        if c & 0xF0 == 0xE0 {
            let mut n = (c & 0x0F) as usize;
            for _ in 0..3 {
                n = (n << 8) + self.read_byte()? as usize;
            }
            return Ok(n);
        }
        let mut n = 0usize;
        for _ in 0..4 {
            n = (n << 8) + self.read_byte()? as usize;
        }
        Ok(n)
    }

    fn read_byte(&mut self) -> io::Result<u8> {
        let mut b = [0u8; 1];
        self.stream.read_exact(&mut b)?;
        Ok(b[0])
    }
}

fn hex_decode(s: &str) -> Vec<u8> {
    (0..s.len())
        .step_by(2)
        .filter_map(|i| s.get(i..i + 2))
        .filter_map(|b| u8::from_str_radix(b, 16).ok())
        .collect()
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

// ---------- конфіг / підключення ----------

pub struct Config {
    pub host: String,
    pub user: String,
    pub pass: String,
}

pub fn home_path(name: &str) -> std::path::PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/Users/vitalii".to_string());
    std::path::Path::new(&home).join(name)
}

fn load_env_file() -> HashMap<String, String> {
    let mut map = HashMap::new();
    if let Ok(content) = std::fs::read_to_string(home_path(".mikrotik.env")) {
        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some(idx) = line.find('=') {
                map.insert(
                    line[..idx].trim().to_string(),
                    line[idx + 1..].trim().to_string(),
                );
            }
        }
    }
    map
}

pub fn load_config() -> Result<Config, String> {
    let env_file = load_env_file();
    let host = std::env::var("MIKROTIK_HOST")
        .ok()
        .or_else(|| env_file.get("MIKROTIK_HOST").cloned())
        .unwrap_or_else(|| "192.168.88.1".to_string());
    let user = std::env::var("MIKROTIK_USER")
        .ok()
        .or_else(|| env_file.get("MIKROTIK_USER").cloned())
        .unwrap_or_else(|| "admin".to_string());
    let pass = std::env::var("MIKROTIK_PASS")
        .ok()
        .or_else(|| env_file.get("MIKROTIK_PASS").cloned())
        .ok_or_else(|| "MIKROTIK_PASS не задано (~/.mikrotik.env або env-змінна)".to_string())?;
    Ok(Config { host, user, pass })
}

pub fn connect_and_login(timeout: Duration) -> Result<ApiRos, String> {
    let cfg = load_config()?;
    let mut api = ApiRos::connect(&cfg.host, timeout).map_err(|e| e.to_string())?;
    if !api.login(&cfg.user, &cfg.pass).map_err(|e| e.to_string())? {
        return Err("login failed".to_string());
    }
    Ok(api)
}

// ---------- допоміжні WAN-функції ----------

/// Instant rx/tx bits-per-second for both WAN interfaces: (zte_rx, zte_tx, soyea_rx, soyea_tx).
pub fn read_traffic(api: &mut ApiRos) -> (Option<i64>, Option<i64>, Option<i64>, Option<i64>) {
    let mut zte = None;
    let mut soyea = None;
    if let Ok(rows) = api.talk(&[
        "/interface/monitor-traffic",
        "=interface=ether1,ether3",
        "=once=",
    ]) {
        for (r, attrs) in rows {
            if r != "!re" {
                continue;
            }
            let rx = attrs
                .get("=rx-bits-per-second")
                .and_then(|v| v.parse::<i64>().ok())
                .unwrap_or(0);
            let tx = attrs
                .get("=tx-bits-per-second")
                .and_then(|v| v.parse::<i64>().ok())
                .unwrap_or(0);
            match attrs.get("=name").map(|s| s.as_str()) {
                Some("ether3") => zte = Some((rx, tx)),
                Some("ether1") => soyea = Some((rx, tx)),
                _ => {}
            }
        }
    }
    (
        zte.map(|(rx, _)| rx),
        zte.map(|(_, tx)| tx),
        soyea.map(|(rx, _)| rx),
        soyea.map(|(_, tx)| tx),
    )
}
