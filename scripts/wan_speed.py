#!/usr/bin/env python3
"""
Миттєва швидкість обох WAN-інтерфейсів (rx/tx bits per second)
через /interface/monitor-traffic. Віддає JSON для десктопного застосунку.
Виклик: python3 ~/wan_speed.py
"""
import socket, hashlib, os, json, sys
from datetime import datetime

HOST_KEY, USER_KEY, PASS_KEY = "MIKROTIK_HOST", "MIKROTIK_USER", "MIKROTIK_PASS"

# WAN-інтерфейси (як у wan_monitor.py): ether3 → ZTE (WAN1), ether1 → Soyea (WAN2)
CHANNEL_BY_IFACE = {"ether3": "zte", "ether1": "soyea"}


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


def main():
    try:
        sk = socket.socket()
        sk.settimeout(15)
        sk.connect((HOST, 8728))
        a = ApiRos(sk)
        if not a.login(USER, PW):
            print(json.dumps({"error": "login failed"}))
            sys.exit(1)
    except Exception as e:
        print(json.dumps({"error": f"connect error: {e}"}))
        sys.exit(1)

    ifaces = ",".join(CHANNEL_BY_IFACE)
    rows = [at for r, at in a.talk(["/interface/monitor-traffic",
                                    "=interface=" + ifaces, "=once="]) if r == '!re']
    sk.close()

    out = {"ts": datetime.now().strftime("%Y-%m-%dT%H:%M:%S")}
    for row in rows:
        chan = CHANNEL_BY_IFACE.get(row.get('=name', ''))
        if not chan:
            continue
        out[chan] = {
            "rx_bps": int(row.get('=rx-bits-per-second', 0) or 0),
            "tx_bps": int(row.get('=tx-bits-per-second', 0) or 0),
        }
    print(json.dumps(out))


if __name__ == "__main__":
    main()
