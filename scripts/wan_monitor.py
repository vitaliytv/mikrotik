#!/usr/bin/env python3
"""
WAN Monitor + якісний авто-failover для MikroTik dual-WAN.
Cron: */3 * * * * /usr/bin/python3 /Users/vitalii/wan_monitor.py
CSV: ~/wan_log.csv   Стан: ~/wan_state.json
"""
import socket, hashlib, time, csv, os, re, json
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
STATE_PATH = os.path.expanduser("~/wan_state.json")
CSV_HEADER = ["timestamp","zte_avg_ms","zte_max_ms","zte_loss_pct",
              "soyea_avg_ms","soyea_max_ms","soyea_loss_pct","zte_active","soyea_active"]

PROBE_ZTE   = "4.2.2.1"    # Level3 DNS — тест через LMT
PROBE_SOYEA = "4.2.2.2"    # Level3 DNS — тест через BITE

RTT_AVG_BAD  = 150.0  # мс — середня вище = погано
RTT_MAX_BAD  = 160.0  # мс — spike вище = погано (важливо для дзвінків)
RTT_AVG_GOOD = 100.0  # мс — середня нижче = відновився
RTT_MAX_GOOD = 110.0  # мс — spike нижче = відновився
LOSS_BAD     = 30.0   # % — вище = dead
BAD_STREAK   = 2      # поспіль поганих → вимкнути (6 хв)
GOOD_STREAK  = 2      # поспіль добрих → ввімкнути (6 хв)


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


def set_wan_routes(a, prefix, enabled):
    """Enable/disable LB-w{prefix}* default routes (0.0.0.0/0 only)."""
    cmd = "/ip/route/enable" if enabled else "/ip/route/disable"
    count = 0
    for row in get_rows(a, "/ip/route/print"):
        c   = row.get('=comment', '')
        dst = row.get('=dst-address', '')
        if c.startswith(f'LB-w{prefix}') and dst == '0.0.0.0/0':
            a.talk([cmd, "=.id=" + row['=.id']])
            count += 1
    return count


def load_state():
    try:
        with open(STATE_PATH) as f:
            return json.load(f)
    except:
        return {
            "zte":   {"disabled": False, "bad_streak": 0, "good_streak": 0},
            "soyea": {"disabled": False, "bad_streak": 0, "good_streak": 0}
        }


def save_state(state):
    with open(STATE_PATH, 'w') as f:
        json.dump(state, f, indent=2)


def manage(a, state, chan, avg_ms, max_ms, loss_pct, other_disabled):
    """Quality-based route management. Returns action description or None."""
    s      = state[chan]
    prefix = "1" if chan == "zte" else "2"

    is_bad  = (avg_ms is None
               or avg_ms  > RTT_AVG_BAD
               or (max_ms is not None and max_ms > RTT_MAX_BAD)
               or loss_pct >= LOSS_BAD)
    is_good = (avg_ms is not None
               and avg_ms  <= RTT_AVG_GOOD
               and (max_ms is None or max_ms <= RTT_MAX_GOOD)
               and loss_pct < LOSS_BAD)

    if not s["disabled"]:
        if is_bad:
            s["bad_streak"]  = min(s["bad_streak"] + 1, BAD_STREAK + 5)
            s["good_streak"] = 0
            if s["bad_streak"] >= BAD_STREAK:
                if other_disabled:
                    return f"деградує але інший канал вже вимкнений — лишаємо"
                n = set_wan_routes(a, prefix, False)
                s["disabled"] = True
                s["bad_streak"] = 0
                return f"ВИМКНЕНО avg>{RTT_AVG_BAD}мс або spike>{RTT_MAX_BAD}мс ({n} маршрутів)"
        else:
            s["bad_streak"] = max(0, s["bad_streak"] - 1)
    else:
        if is_good:
            s["good_streak"]  = min(s["good_streak"] + 1, GOOD_STREAK + 5)
            s["bad_streak"]   = 0
            if s["good_streak"] >= GOOD_STREAK:
                n = set_wan_routes(a, prefix, True)
                s["disabled"]    = False
                s["good_streak"] = 0
                return f"ВІДНОВЛЕНО avg<{RTT_AVG_GOOD}мс spike<{RTT_MAX_GOOD}мс ({n} маршрутів)"
        else:
            s["good_streak"] = 0
    return None


def update_voip_routing(a, better_prefix, state):
    """Point VOIP:route mangle rule to the currently better WAN."""
    target = f"to_WAN{better_prefix}"
    last   = state.get("voip_wan", "")
    if last == target:
        return None  # no change needed
    for row in get_rows(a, "/ip/firewall/mangle/print"):
        if row.get('=comment') == 'VOIP:route':
            a.talk(["/ip/firewall/mangle/set", "=.id=" + row['=.id'],
                    "=new-routing-mark=" + target])
            state["voip_wan"] = target
            return f"VOIP дзвінки → {target} (було: {last or '?'})"
    return None  # rule not found (not set up yet)


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

    ts    = datetime.now().strftime("%Y-%m-%dT%H:%M:%S")
    state = load_state()

    # Measure both channels
    zte_avg,   zte_max,   zte_loss   = ping_via_gw(a, PROBE_ZTE,   gw1) if gw1 else (None, None, 100.0)
    soyea_avg, soyea_max, soyea_loss = ping_via_gw(a, PROBE_SOYEA, gw2) if gw2 else (None, None, 100.0)

    # Quality management (LMT first, then BITE checks if LMT was just disabled)
    zte_action   = manage(a, state, "zte",   zte_avg,   zte_max,   zte_loss,   state["soyea"]["disabled"])
    soyea_action = manage(a, state, "soyea", soyea_avg, soyea_max, soyea_loss, state["zte"]["disabled"])

    # Emergency: if the ONLY active channel is also bad → re-enable the disabled one.
    # Two bad channels > one bad channel (more aggregate bandwidth, load distribution).
    def is_bad_reading(avg, mx, loss):
        return (avg is None or avg > RTT_AVG_BAD
                or (mx is not None and mx > RTT_MAX_BAD)
                or loss >= LOSS_BAD)

    if state["zte"]["disabled"] and not state["soyea"]["disabled"]:
        if is_bad_reading(soyea_avg, soyea_max, soyea_loss):
            n = set_wan_routes(a, "1", True)
            state["zte"]["disabled"] = False
            state["zte"]["bad_streak"] = 0
            zte_action = f"АВАРІЙНЕ ВІДНОВЛЕННЯ: BITE деградує → LMT повернено ({n} маршрутів)"

    elif state["soyea"]["disabled"] and not state["zte"]["disabled"]:
        if is_bad_reading(zte_avg, zte_max, zte_loss):
            n = set_wan_routes(a, "2", True)
            state["soyea"]["disabled"] = False
            state["soyea"]["bad_streak"] = 0
            soyea_action = f"АВАРІЙНЕ ВІДНОВЛЕННЯ: LMT деградує → BITE повернено ({n} маршрутів)"

    voip_action  = None  # VOIP routing is static per-call — no dynamic updates (prevents mid-call IP change)
    zte_active   = 0 if state["zte"]["disabled"]   else 1
    soyea_active = 0 if state["soyea"]["disabled"] else 1

    save_state(state)
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
    if zte_action:   print(f"  LMT   → {zte_action}")
    if soyea_action: print(f"  BITE  → {soyea_action}")
    if voip_action:  print(f"  VOIP  → {voip_action}")


if __name__ == "__main__":
    main()
