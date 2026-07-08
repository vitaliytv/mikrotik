#!/usr/bin/env python3
"""
Читає системний лог MikroTik напряму (netwatch/route/dhcp/link події)
і віддає структурований JSON для десктопного застосунку.
Виклик: python3 ~/wan_router_log.py
"""
import socket, hashlib, os, re, json, sys

HOST_KEY, USER_KEY, PASS_KEY = "MIKROTIK_HOST", "MIKROTIK_USER", "MIKROTIK_PASS"


def _load_env_file(path="~/.mikrotik.env"):
    d = {}
    try:
        with open(os.path.expanduser(path)) as f:
            for line in f:
                line = line.strip()
                if not line or line.startswith('#') or '=' not in line:
                    continue
                k, v = line.split('=', 1)
                d[k.strip()] = v.strip()
    except FileNotFoundError:
        pass
    return d


_ENV_FILE = _load_env_file()
HOST = os.environ.get(HOST_KEY) or _ENV_FILE.get(HOST_KEY, "192.168.88.1")
USER = os.environ.get(USER_KEY) or _ENV_FILE.get(USER_KEY, "admin")
PW   = os.environ.get(PASS_KEY) or _ENV_FILE.get(PASS_KEY)
if not PW:
    print(json.dumps({"error": "MIKROTIK_PASS не задано (~/.mikrotik.env або env-змінна)"}))
    sys.exit(1)


class ApiRos:
    def __init__(self, sk): self.sk = sk
    def login(self, u, p):
        for r, a in self.talk(["/login", "=name="+u, "=password="+p]):
            if r == '!trap': return False
            if '=ret' in a:
                t = bytes.fromhex(a['=ret']); m = hashlib.md5()
                m.update(b'\x00' + p.encode() + t)
                self.talk(["/login", "=name="+u, "=response=00"+m.hexdigest()])
        return True
    def talk(self, words):
        self.w(words); r = []
        while True:
            s = self.r()
            if not s: continue
            rep = s[0]; at = {}
            for x in s[1:]:
                j = x.find('=', 1); at[x if j < 0 else x[:j]] = '' if j < 0 else x[j+1:]
            r.append((rep, at))
            if rep == '!done': return r
    def w(self, words):
        for x in words: self.ww(x)
        self.wl(0)
    def r(self):
        o = []
        while True:
            w = self.rw()
            if w == '': return o
            o.append(w)
    def ww(self, w): self.wl(len(w)); self.sk.sendall(w.encode('latin-1'))
    def rw(self):
        n = self.rl(); return self.rs(n).decode('latin-1', 'replace') if n else ''
    def wl(self, l):
        if l < 0x80: b = bytes([l])
        elif l < 0x4000: b = (l | 0x8000).to_bytes(2, 'big')
        elif l < 0x200000: b = (l | 0xC00000).to_bytes(3, 'big')
        elif l < 0x10000000: b = (l | 0xE0000000).to_bytes(4, 'big')
        else: b = bytes([0xF0]) + l.to_bytes(4, 'big')
        self.sk.sendall(b)
    def rl(self):
        c = self.rs(1)[0]
        if c & 0x80 == 0: return c
        if c & 0xC0 == 0x80: return ((c & ~0xC0) << 8) + self.rs(1)[0]
        if c & 0xE0 == 0xC0:
            n = c & ~0xE0
            for _ in range(2): n = (n << 8) + self.rs(1)[0]
            return n
        if c & 0xF0 == 0xE0:
            n = c & ~0xF0
            for _ in range(3): n = (n << 8) + self.rs(1)[0]
            return n
        n = 0
        for _ in range(4): n = (n << 8) + self.rs(1)[0]
        return n
    def rs(self, n):
        d = b''
        while len(d) < n:
            x = self.sk.recv(n - len(d))
            if not x: raise RuntimeError("closed")
            d += x
        return d


def get_rows(a, cmd, extra=None):
    words = [cmd] + (extra or [])
    return [at for r, at in a.talk(words) if r == '!re']


FLAP_RE = re.compile(
    r'^route .* changed by netwatch:type: icmp, host: ([\d.]+)/action:(\d+) '
    r'\(.*disabled=(yes|no)'
)

CHANNEL_BY_HOST = {"8.8.8.8": "zte", "1.1.1.1": "soyea"}


def main():
    try:
        sk = socket.socket()
        sk.settimeout(20)
        sk.connect((HOST, 8728))
        a = ApiRos(sk)
        if not a.login(USER, PW):
            print(json.dumps({"error": "login failed"}))
            sys.exit(1)
    except Exception as e:
        print(json.dumps({"error": f"connect error: {e}"}))
        sys.exit(1)

    # Поточний стан netwatch
    netwatch = []
    for row in get_rows(a, "/tool/netwatch/print"):
        c = row.get('=comment', '')
        if c.startswith('LBnw'):
            netwatch.append({
                "comment": c,
                "host": row.get('=host', ''),
                "channel": CHANNEL_BY_HOST.get(row.get('=host', ''), '?'),
                "status": row.get('=status', '?'),
                "since": row.get('=since', ''),
                "interval": row.get('=interval', ''),
                "packet_count": row.get('=packet-count', ''),
                "thr_loss_percent": row.get('=thr-loss-percent', ''),
            })

    # Активні маршрути LB-w*
    routes = []
    for row in get_rows(a, "/ip/route/print"):
        c = row.get('=comment', '')
        if c.startswith('LB-w') and row.get('=dst-address') == '0.0.0.0/0':
            routes.append({
                "comment": c,
                "active": row.get('=active', ''),
                "disabled": row.get('=disabled', ''),
            })

    # Системний лог — flap-події + інші WAN-релевантні рядки
    log_rows = get_rows(a, "/log/print")
    flap_events = []
    seen_actions = set()
    other_events = []
    keywords = ('dhcp', 'ether1', 'ether3', 'link', 'ppp')
    for row in log_rows:
        msg = row.get('=message', '')
        t = row.get('=time', '')
        m = FLAP_RE.match(msg)
        if m:
            host, action_id, disabled = m.group(1), m.group(2), m.group(3)
            key = (t, action_id)
            if key in seen_actions:
                continue
            seen_actions.add(key)
            flap_events.append({
                "time": t,
                "channel": CHANNEL_BY_HOST.get(host, '?'),
                "host": host,
                "action": "down" if disabled == "yes" else "up",
            })
        elif any(k in msg.lower() for k in keywords):
            other_events.append({"time": t, "message": msg})

    sk.close()

    print(json.dumps({
        "netwatch": netwatch,
        "routes": routes,
        "flap_events": flap_events,
        "other_events": other_events[-100:],
        "log_total_lines": len(log_rows),
    }))


if __name__ == "__main__":
    main()
