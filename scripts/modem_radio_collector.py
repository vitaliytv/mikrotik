#!/usr/bin/env python3
"""Collect modem radio metrics and persist them in the RouterOS disk log."""

from __future__ import annotations

import argparse
import base64
import hashlib
import http.cookiejar
import json
import os
import plistlib
import re
import shutil
import socket
import subprocess
import sys
import urllib.error
import urllib.parse
import urllib.request
import xml.etree.ElementTree as ET
from pathlib import Path

ROUTER_HOST = "192.168.88.1"
ROUTER_PORT = 8728
MODEMS = {
    "lmt": ("192.168.0.1", "ai.vitalii.mymikrotik.modem.lmt"),
    "bite": ("192.168.8.1", "ai.vitalii.mymikrotik.modem.bite"),
}
SAFE_VALUE = re.compile(r"[^A-Za-z0-9.+:/_-]+")
NUMBER = re.compile(r"-?\d+(?:\.\d+)?")


class CollectorError(RuntimeError):
    """A safe, user-facing collection error without sensitive response data."""


def encode_length(length: int) -> bytes:
    if length < 0x80:
        return bytes([length])
    if length < 0x4000:
        length |= 0x8000
        return bytes([(length >> 8) & 0xFF, length & 0xFF])
    if length < 0x200000:
        length |= 0xC00000
        return bytes([(length >> 16) & 0xFF, (length >> 8) & 0xFF, length & 0xFF])
    if length < 0x10000000:
        length |= 0xE0000000
        return bytes(
            [(length >> 24) & 0xFF, (length >> 16) & 0xFF, (length >> 8) & 0xFF, length & 0xFF]
        )
    return b"\xF0" + length.to_bytes(4, "big")


def read_length(connection: socket.socket) -> int:
    first = connection.recv(1)
    if not first:
        raise CollectorError("RouterOS закрив API connection")
    value = first[0]
    if value & 0x80 == 0:
        return value
    if value & 0xC0 == 0x80:
        return ((value & 0x3F) << 8) | _read_exact(connection, 1)[0]
    if value & 0xE0 == 0xC0:
        return ((value & 0x1F) << 16) | int.from_bytes(_read_exact(connection, 2), "big")
    if value & 0xF0 == 0xE0:
        return ((value & 0x0F) << 24) | int.from_bytes(_read_exact(connection, 3), "big")
    return int.from_bytes(_read_exact(connection, 4), "big")


def _read_exact(connection: socket.socket, length: int) -> bytes:
    data = bytearray()
    while len(data) < length:
        chunk = connection.recv(length - len(data))
        if not chunk:
            raise CollectorError("RouterOS закрив API connection")
        data.extend(chunk)
    return bytes(data)


def routeros_talk(connection: socket.socket, words: list[str]) -> list[list[str]]:
    payload = b"".join(encode_length(len(word.encode())) + word.encode() for word in words) + b"\x00"
    connection.sendall(payload)
    replies: list[list[str]] = []
    sentence: list[str] = []
    while True:
        length = read_length(connection)
        if length == 0:
            replies.append(sentence)
            if sentence and sentence[0] in {"!done", "!fatal"}:
                return replies
            sentence = []
            continue
        sentence.append(_read_exact(connection, length).decode(errors="replace"))


def router_credentials() -> tuple[str, str, str]:
    values: dict[str, str] = {}
    env_path = Path.home() / ".mikrotik.env"
    if env_path.exists():
        for line in env_path.read_text(encoding="utf-8").splitlines():
            key, separator, value = line.partition("=")
            if separator and key.strip() in {"MIKROTIK_HOST", "MIKROTIK_USER", "MIKROTIK_PASS"}:
                values[key.strip()] = value.strip().strip("'\"")
    for key in ("MIKROTIK_HOST", "MIKROTIK_USER", "MIKROTIK_PASS"):
        if os.environ.get(key):
            values[key] = os.environ[key]
    user = values.get("MIKROTIK_USER")
    password = values.get("MIKROTIK_PASS")
    if not user or not password:
        raise CollectorError("MIKROTIK_USER/MIKROTIK_PASS відсутні у ~/.mikrotik.env")
    return values.get("MIKROTIK_HOST", ROUTER_HOST), user, password


def router_log(message: str) -> None:
    host, user, password = router_credentials()
    with socket.create_connection((host, ROUTER_PORT), timeout=8) as connection:
        connection.settimeout(8)
        login = routeros_talk(connection, ["/login", f"=name={user}", f"=password={password}"])
        if any(sentence and sentence[0] == "!trap" for sentence in login):
            raise CollectorError("RouterOS API login failed")
        result = routeros_talk(connection, ["/log/info", f"=message={message}"])
        if any(sentence and sentence[0] in {"!trap", "!fatal"} for sentence in result):
            raise CollectorError("RouterOS відхилив log record")


def keychain_password(service: str) -> str:
    result = subprocess.run(
        ["/usr/bin/security", "find-generic-password", "-w", "-a", "admin", "-s", service],
        check=False,
        capture_output=True,
        text=True,
    )
    if result.returncode != 0 or not result.stdout.rstrip("\n"):
        raise CollectorError(f"password відсутній у Keychain service={service}")
    return result.stdout.rstrip("\n")


def opener() -> urllib.request.OpenerDirector:
    return urllib.request.build_opener(urllib.request.HTTPCookieProcessor(http.cookiejar.CookieJar()))


def http_text(
    client: urllib.request.OpenerDirector,
    url: str,
    *,
    data: bytes | None = None,
    headers: dict[str, str] | None = None,
) -> tuple[str, dict[str, str]]:
    request = urllib.request.Request(url, data=data, headers=headers or {})
    try:
        with client.open(request, timeout=8) as response:
            return response.read().decode(errors="replace"), dict(response.headers.items())
    except (urllib.error.URLError, TimeoutError, OSError) as error:
        raise CollectorError("modem HTTP request failed") from error


def zte_password_hash(password: str, login_digest: str) -> str:
    first = hashlib.sha256(password.encode()).hexdigest()
    return hashlib.sha256(f"{first}{login_digest}".encode()).hexdigest()


def collect_zte(host: str, password: str) -> dict[str, str]:
    client = opener()
    base = f"http://{host}"
    login_text, _ = http_text(client, f"{base}/goform/goform_get_cmd_process?isTest=false&cmd=LD")
    login_digest = str(json.loads(login_text).get("LD", ""))
    body = urllib.parse.urlencode(
        {"isTest": "false", "goformId": "LOGIN", "password": zte_password_hash(password, login_digest)}
    ).encode()
    auth_text, _ = http_text(
        client,
        f"{base}/goform/goform_set_cmd_process",
        data=body,
        headers={"Content-Type": "application/x-www-form-urlencoded; charset=UTF-8"},
    )
    if str(json.loads(auth_text).get("result")) != "0":
        raise CollectorError("LMT modem login failed")
    commands = ",".join(
        [
            "network_type", "lte_rsrp", "lte_rsrq", "lte_snr", "Z5g_rsrp", "Z5g_rsrq",
            "Z5g_SINR", "ZCELLINFO_band", "lte_ca_pcell_band", "lte_ca_scell_band",
            "wan_lte_ca", "lte_pci", "nr5g_pci", "cell_id", "nr5g_cell_id",
            "wan_active_band", "nr5g_action_band", "signalbar",
        ]
    )
    signal_text, _ = http_text(
        client, f"{base}/goform/goform_get_cmd_process?isTest=false&multi_data=1&cmd={commands}"
    )
    raw = {key: str(value) for key, value in json.loads(signal_text).items()}
    is_5g = bool(raw.get("Z5g_rsrp") or raw.get("nr5g_cell_id"))
    values = {
        "model": "MC888",
        "operator": "LMT",
        "network": raw.get("network_type", ""),
        "cell_id": raw.get("nr5g_cell_id" if is_5g else "cell_id", ""),
        "pci": raw.get("nr5g_pci" if is_5g else "lte_pci", ""),
        "band": raw.get("nr5g_action_band" if is_5g else "wan_active_band", "")
        or raw.get("ZCELLINFO_band", "")
        or raw.get("lte_ca_pcell_band", ""),
        "rsrp_dbm": metric(raw.get("Z5g_rsrp" if is_5g else "lte_rsrp")),
        "rsrq_db": metric(raw.get("Z5g_rsrq" if is_5g else "lte_rsrq")),
        "sinr_db": metric(raw.get("Z5g_SINR" if is_5g else "lte_snr")),
        "signal_bars": metric(raw.get("signalbar")),
        "ca": raw.get("wan_lte_ca", "") or raw.get("lte_ca_scell_band", ""),
    }
    if not any(values.get(key) for key in ("cell_id", "pci", "rsrp_dbm")):
        raise CollectorError("LMT modem radio metrics missing")
    return values


def huawei_password_hash(username: str, password: str, token: str) -> str:
    inner = base64.b64encode(hashlib.sha256(password.encode()).digest()).decode()
    return base64.b64encode(hashlib.sha256(f"{username}{inner}{token}".encode()).digest()).decode()


def xml_values(document: str) -> dict[str, str]:
    root = ET.fromstring(document)
    return {node.tag.split("}")[-1]: (node.text or "").strip() for node in root.iter() if len(node) == 0}


def collect_huawei(host: str, username: str, password: str) -> dict[str, str]:
    client = opener()
    base = f"http://{host}"
    session_text, _ = http_text(client, f"{base}/api/webserver/SesTokInfo")
    session = xml_values(session_text)
    token = session.get("TokInfo", "")
    if not token:
        raise CollectorError("BITE modem session token missing")
    login_xml = (
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>"
        f"<request><Username>{username}</Username>"
        f"<Password>{huawei_password_hash(username, password, token)}</Password>"
        "<password_type>4</password_type></request>"
    ).encode()
    login_text, login_headers = http_text(
        client,
        f"{base}/api/user/login",
        data=login_xml,
        headers={"Content-Type": "application/xml", "__RequestVerificationToken": token},
    )
    if "OK" not in login_text:
        raise CollectorError("BITE modem login failed")
    request_token = login_headers.get("__RequestVerificationTokenone") or login_headers.get(
        "__RequestVerificationToken"
    )
    headers = {"__RequestVerificationToken": request_token} if request_token else {}
    signal_text, _ = http_text(client, f"{base}/api/device/signal", headers=headers)
    signal = xml_values(signal_text)
    status_text, _ = http_text(client, f"{base}/api/monitoring/status", headers=headers)
    status = xml_values(status_text)
    basic_text, _ = http_text(client, f"{base}/api/device/basic_information", headers=headers)
    basic = xml_values(basic_text)
    plmn_text, _ = http_text(client, f"{base}/api/net/current-plmn", headers=headers)
    plmn = xml_values(plmn_text)
    network_code = signal.get("mode") or status.get("CurrentNetworkTypeEx") or plmn.get("Rat") or "LTE"
    network = {"19": "LTE", "1011": "LTE_CA"}.get(network_code, network_code)
    values = {
        "model": basic.get("devicename") or basic.get("DeviceName") or "B628-350",
        "operator": plmn.get("FullName") or "BITE",
        "network": network,
        "cell_id": signal.get("cell_id", ""),
        "pci": signal.get("pci", ""),
        "band": signal.get("band", ""),
        "rsrp_dbm": metric(signal.get("rsrp")),
        "rsrq_db": metric(signal.get("rsrq")),
        "sinr_db": metric(signal.get("sinr")),
        "signal_bars": metric(status.get("SignalIcon")),
        "ca": "active" if network_code == "1011" else "inactive",
    }
    if not any(values.get(key) for key in ("cell_id", "pci", "rsrp_dbm")):
        raise CollectorError("BITE modem radio metrics missing")
    return values


def metric(value: str | None) -> str:
    match = NUMBER.search(value or "")
    return match.group(0) if match else ""


def safe(value: object) -> str:
    return SAFE_VALUE.sub("_", str(value).strip()).strip("_") or "unknown"


def radio_log_line(wan: str, status: str, values: dict[str, str] | None = None) -> str:
    fields: list[tuple[str, object]] = [("type", "radio"), ("wan", wan), ("status", status)]
    for key in (
        "model", "operator", "network", "cell_id", "pci", "band", "rsrp_dbm", "rsrq_db",
        "sinr_db", "signal_bars", "ca",
    ):
        value = (values or {}).get(key)
        if value not in (None, ""):
            fields.append((key, value))
    return "WANQUALITY " + " ".join(f"{key}={safe(value)}" for key, value in fields)


def collect_once(*, dry_run: bool = False) -> int:
    results: list[str] = []
    failures = 0
    for wan, (host, service) in MODEMS.items():
        try:
            password = keychain_password(service)
            values = collect_zte(host, password) if wan == "lmt" else collect_huawei(host, "admin", password)
            line = radio_log_line(wan, "up", values)
        except (CollectorError, ValueError, json.JSONDecodeError, ET.ParseError):
            failures += 1
            line = radio_log_line(wan, "error")
        results.append(line)
        if not dry_run:
            router_log(line)
    if dry_run:
        print("\n".join(results))
    return 1 if failures else 0


def install() -> None:
    router_credentials()
    for wan, (_, service) in MODEMS.items():
        print(f"Введіть admin password для {wan.upper()} modem (Keychain prompt):")
        subprocess.run(
            ["/usr/bin/security", "add-generic-password", "-U", "-a", "admin", "-s", service, "-w"],
            check=True,
        )
    if collect_once(dry_run=False) != 0:
        raise CollectorError("Не вдалося прочитати обидва modem; LaunchAgent не встановлено")
    support = Path.home() / "Library/Application Support/MyMikroTik"
    support.mkdir(parents=True, exist_ok=True)
    installed_script = support / "modem_radio_collector.py"
    shutil.copy2(Path(__file__).resolve(), installed_script)
    plist_path = Path.home() / "Library/LaunchAgents/ai.vitalii.mymikrotik.modem-radio.plist"
    plist = {
        "Label": "ai.vitalii.mymikrotik.modem-radio",
        "ProgramArguments": [shutil.which("python3") or sys.executable, str(installed_script)],
        "RunAtLoad": True,
        "StartInterval": 300,
        "StandardOutPath": "/dev/null",
        "StandardErrorPath": "/dev/null",
    }
    plist_path.parent.mkdir(parents=True, exist_ok=True)
    with plist_path.open("wb") as file:
        plistlib.dump(plist, file)
    domain = f"gui/{os.getuid()}"
    subprocess.run(["/bin/launchctl", "bootout", domain, str(plist_path)], check=False, capture_output=True)
    subprocess.run(["/bin/launchctl", "bootstrap", domain, str(plist_path)], check=True)
    subprocess.run(
        ["/bin/launchctl", "kickstart", "-k", f"{domain}/ai.vitalii.mymikrotik.modem-radio"], check=True
    )
    print(f"Collector встановлено: кожні 5 хвилин, {plist_path}")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--install", action="store_true", help="save modem passwords and install LaunchAgent")
    parser.add_argument("--dry-run", action="store_true", help="print sanitized metrics without writing RouterOS log")
    args = parser.parse_args()
    try:
        if args.install:
            install()
            return 0
        return collect_once(dry_run=args.dry_run)
    except (CollectorError, subprocess.CalledProcessError) as error:
        print(f"Collector error: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
