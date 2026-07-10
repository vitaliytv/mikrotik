#!/usr/bin/env python3
"""
WAN Monitor для MikroTik dual-WAN.
Cron: */3 * * * * /usr/bin/python3 /Users/vitalii/wan_monitor.py
CSV: ~/wan_log.csv
"""
import socket, hashlib, time, csv, os, re
from datetime import datetime

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
HOST = os.environ.get("MIKROTIK_HOST") or _ENV_FILE.get("MIKROTIK_HOST", "192.168.88.1")
USER = os.environ.get("MIKROTIK_USER") or _ENV_FILE.get("MIKROTIK_USER", "admin")
PW   = os.environ.get("MIKROTIK_PASS") or _ENV_FILE.get("MIKROTIK_PASS")
if not PW:
    raise SystemExit("MIKROTIK_PASS не задано. Створи ~/.mikrotik.env з MIKROTIK_PASS=... або встанови env-змінну.")
CSV_PATH   = os.path.expanduser("~/wan_log.csv")
CSV_HEADER = ["timestamp","zte_avg_ms","zte_max_ms","zte_loss_pct",
              "soyea_avg_ms","soyea_max_ms","soyea_loss_pct","zte_active","soyea_active"]

PROBE_ZTE   = "4.2.2.1"    # Level3 DNS — тест через LMT
PROBE_SOYEA = "4.2.2.2"    # Level3 DNS — тест через BITE


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


def parse_rtt(val):
    if not val or val in ('?', ''): return None
    val = str(val).strip()
    m = re.match(r'^(?:(\d+)ms)?(?:(\d+)us)?$', val)
    if m and (m.group(1) or m.group(2)):
        return round(float(m.group(1) or 0) + float(m.group(2) or 0) / 1000, 2)
    if val.endswith('ms'): return round(float(val[:-2]), 2)
    if val.endswith('us'): return round(float(val[:-2]) / 1000, 2)
    if val.endswith('s'):  return round(float(val[:-1]) * 1000, 2)
    try: return round(float(val) / 1000, 2)
    except: return None


def get_rows(a, cmd):
    return [at for r, at in a.talk([cmd]) if r == '!re']


def ping_via_gw(a, probe_ip, gw, count=5):
    tag = "MON" + probe_ip[-3:].replace('.', '')
    for row in get_rows(a, "/ip/route/print"):
        if row.get('=comment') == tag:
            a.talk(["/ip/route/remove", "=.id=" + row['=.id']])
    a.talk(["/ip/route/add", "=dst-address=" + probe_ip + "/32",
            "=gateway=" + gw, "=comment=" + tag])
    time.sleep(0.5)
    try:
        rs = [at for r, at in a.talk(["/ping", "=address=" + probe_ip,
                                       "=count=" + str(count)]) if r == '!re']
        if rs:
            last = rs[-1]
            avg  = parse_rtt(last.get('=avg-rtt'))
            mx   = parse_rtt(last.get('=max-rtt'))
            loss = float(last.get('=packet-loss', '100'))
        else:
            avg, mx, loss = None, None, 100.0
    except:
        avg, mx, loss = None, None, 100.0
    for row in get_rows(a, "/ip/route/print"):
        if row.get('=comment') == tag:
            a.talk(["/ip/route/remove", "=.id=" + row['=.id']])
    return avg, mx, loss


def wan_routes_active(a, prefix):
    """Return whether any LB-w{prefix} default route is currently enabled."""
    found = False
    for row in get_rows(a, "/ip/route/print"):
        c   = row.get('=comment', '')
        dst = row.get('=dst-address', '')
        if c.startswith(f'LB-w{prefix}') and dst == '0.0.0.0/0':
            found = True
            if row.get('=disabled', '').lower() not in ('true', 'yes'):
                return True
    return False if found else None


def main():
    try:
        sk = socket.socket()
        sk.settimeout(30)
        sk.connect((HOST, 8728))
        a = ApiRos(sk)
        if not a.login(USER, PW):
            print("login failed"); return
    except Exception as e:
        print(f"connect error: {e}"); return

    # WAN gateways
    gw1 = gw2 = None
    for row in get_rows(a, "/ip/dhcp-client/print"):
        iface, gw = row.get('=interface', ''), row.get('=gateway', '')
        if iface == 'ether3' and gw: gw1 = gw
        if iface == 'ether1' and gw: gw2 = gw

    ts = datetime.now().strftime("%Y-%m-%dT%H:%M:%S")

    # Measure both channels
    zte_avg,   zte_max,   zte_loss   = ping_via_gw(a, PROBE_ZTE,   gw1) if gw1 else (None, None, 100.0)
    soyea_avg, soyea_max, soyea_loss = ping_via_gw(a, PROBE_SOYEA, gw2) if gw2 else (None, None, 100.0)

    zte_active   = 1 if wan_routes_active(a, "1") else 0
    soyea_active = 1 if wan_routes_active(a, "2") else 0
    sk.close()

    # Write CSV
    write_header = not os.path.exists(CSV_PATH)
    with open(CSV_PATH, 'a', newline='') as f:
        w = csv.writer(f)
        if write_header: w.writerow(CSV_HEADER)
        w.writerow([ts, zte_avg, zte_max, zte_loss,
                    soyea_avg, soyea_max, soyea_loss,
                    zte_active, soyea_active])

    def fmt(name, avg, loss, active):
        q  = f"{avg}мс/loss={loss}%" if avg is not None else f"НЕДОСТУПНИЙ"
        st = "" if active else " [ВИМКНЕНО]"
        return f"{name}:{q}{st}"

    print(f"{ts}  {fmt('LMT',zte_avg,zte_loss,zte_active)}  {fmt('BITE',soyea_avg,soyea_loss,soyea_active)}")


if __name__ == "__main__":
    main()
