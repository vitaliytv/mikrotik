#!/usr/bin/env python3
# Полагодити + увімкнути СПРАВЖНІЙ авто-failover на MikroTik (192.168.88.1).
# Запускати, коли Mac на Wi-Fi "UneasyM".
#   python3 ~/fix_mikrotik.py
import socket, time, hashlib, os

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
PROBE1, PROBE2 = "8.8.8.8", "1.1.1.1"   # 8.8.8.8 тест через WAN1(ZTE), 1.1.1.1 через WAN2(Soyea)

class ApiRos:
    def __init__(self, sk): self.sk=sk
    def login(self,u,p):
        for r,a in self.talk(["/login","=name="+u,"=password="+p]):
            if r=='!trap': return False
            if '=ret' in a:
                t=bytes.fromhex(a['=ret']); m=hashlib.md5(); m.update(b'\x00'+p.encode()+t)
                self.talk(["/login","=name="+u,"=response=00"+m.hexdigest()])
        return True
    def talk(self,words):
        self.w(words); r=[]
        while True:
            s=self.r()
            if not s: continue
            rep=s[0]; at={}
            for x in s[1:]:
                j=x.find('=',1); at[x if j<0 else x[:j]]='' if j<0 else x[j+1:]
            r.append((rep,at))
            if rep=='!done': return r
    def w(self,words):
        for x in words: self.ww(x)
        self.wl(0)
    def r(self):
        o=[]
        while True:
            w=self.rw()
            if w=='': return o
            o.append(w)
    def ww(self,w): self.wl(len(w)); self.sk.sendall(w.encode('latin-1'))
    def rw(self):
        n=self.rl(); return self.rs(n).decode('latin-1','replace') if n else ''
    def wl(self,l):
        if l<0x80: b=bytes([l])
        elif l<0x4000: b=(l|0x8000).to_bytes(2,'big')
        elif l<0x200000: b=(l|0xC00000).to_bytes(3,'big')
        elif l<0x10000000: b=(l|0xE0000000).to_bytes(4,'big')
        else: b=bytes([0xF0])+l.to_bytes(4,'big')
        self.sk.sendall(b)
    def rl(self):
        c=self.rs(1)[0]
        if c&0x80==0: return c
        if c&0xC0==0x80: return ((c&~0xC0)<<8)+self.rs(1)[0]
        if c&0xE0==0xC0:
            n=c&~0xE0
            for _ in range(2): n=(n<<8)+self.rs(1)[0]
            return n
        if c&0xF0==0xE0:
            n=c&~0xF0
            for _ in range(3): n=(n<<8)+self.rs(1)[0]
            return n
        n=0
        for _ in range(4): n=(n<<8)+self.rs(1)[0]
        return n
    def rs(self,n):
        d=b''
        while len(d)<n:
            x=self.sk.recv(n-len(d))
            if not x: raise RuntimeError("closed")
            d+=x
        return d

def connect(t=12):
    s=socket.socket(); s.settimeout(t); s.connect((HOST,8728)); a=ApiRos(s)
    if not a.login(USER,PW): raise RuntimeError("login failed")
    return s,a
def rows(a,p): return [at for r,at in a.talk([p]) if r=='!re']
def talk(a,w):
    tr=None
    for r,at in a.talk(w):
        if r=='!trap': tr=at.get('=message')
    return tr

print("Підключаюсь до MikroTik 192.168.88.1 (до 90с)...")
a=None
for i in range(30):
    try: s,a=connect(); print("  з'єднано."); break
    except Exception as e: print(f"  спроба {i+1}: {e}; чекаю 3с"); time.sleep(3)
if a is None:
    print("НЕ ВДАЛОСЯ. Переконайся, що Mac на Wi-Fi 'UneasyM'."); raise SystemExit(1)

# шлюзи WAN
gw={}
for r in rows(a,"/ip/dhcp-client/print"):
    if r.get('=interface') in ('ether1','ether3') and r.get('=status')=='bound' and r.get('=gateway'):
        gw[r['=interface']]=r['=gateway']
if 'ether1' not in gw or 'ether3' not in gw:
    print("  renew DHCP, чекаю 8с...");
    for r in rows(a,"/ip/dhcp-client/print"): a.talk(["/ip/dhcp-client/renew","=.id="+r['=.id']])
    time.sleep(8); gw={}
    for r in rows(a,"/ip/dhcp-client/print"):
        if r.get('=interface') in ('ether1','ether3') and r.get('=status')=='bound' and r.get('=gateway'):
            gw[r['=interface']]=r['=gateway']
gw1=gw.get('ether3'); gw2=gw.get('ether1')   # WAN1=ZTE, WAN2=Soyea
print(f"WAN1 ZTE gw={gw1} | WAN2 Soyea gw={gw2}")
if not gw1 and not gw2: print("Жоден канал без шлюзу — перевір кабелі."); raise SystemExit(1)

# прибрати старе
for r in rows(a,"/ip/route/print"):
    if r.get('=comment','').startswith(('LB-','TEST')): a.talk(["/ip/route/remove","=.id="+r['=.id']])
for r in rows(a,"/tool/netwatch/print"):
    if r.get('=comment','').startswith('LB'): a.talk(["/tool/netwatch/remove","=.id="+r['=.id']])

# /32 проби (пін: 8.8.8.8 -> WAN1, 1.1.1.1 -> WAN2) + повний набір LB-маршрутів
def addroute(c,g,dist,table=None,dst="0.0.0.0/0",extra=None):
    w=["/ip/route/add","=dst-address="+dst,"=gateway="+g,"=distance="+str(dist),"=comment="+c]
    if dst=="0.0.0.0/0": w.append("=check-gateway=ping")
    if table: w.append("=routing-table="+table)
    if extra: w+=extra
    talk(a,w)
if gw1:
    addroute("LB-nw1", gw1, 1, dst=PROBE1+"/32", extra=["=scope=10"])
    addroute("LB-w1m", gw1, 1); addroute("LB-w1t1", gw1,1,"to_WAN1"); addroute("LB-w1t2b", gw1,2,"to_WAN2")
if gw2:
    addroute("LB-nw2", gw2, 1, dst=PROBE2+"/32", extra=["=scope=10"])
    addroute("LB-w2m", gw2, 2); addroute("LB-w2t2", gw2,1,"to_WAN2"); addroute("LB-w2t1b", gw2,2,"to_WAN1")

# netwatch авто-failover
up1='/ip route enable [find comment~"^LB-w1"]'
dn1='/ip route disable [find comment~"^LB-w1"]'
up2='/ip route enable [find comment~"^LB-w2"]'
dn2='/ip route disable [find comment~"^LB-w2"]'
nw_ok=True
nwbase=["=type=icmp","=interval=10s","=timeout=2s","=packet-count=5","=thr-loss-percent=30"]
if gw1:
    t=talk(a,["/tool/netwatch/add","=host="+PROBE1,"=comment=LBnw1","=up-script="+up1,"=down-script="+dn1]+nwbase)
    if t: print("  netwatch WAN1 TRAP:",t); nw_ok=False
if gw2 and nw_ok:
    t=talk(a,["/tool/netwatch/add","=host="+PROBE2,"=comment=LBnw2","=up-script="+up2,"=down-script="+dn2]+nwbase)
    if t: print("  netwatch WAN2 TRAP:",t); nw_ok=False

if nw_ok:
    print("✅ netwatch авто-failover увімкнено. Чекаю 14с на першу перевірку...")
    time.sleep(14)
else:
    print("⚠️ netwatch недоступний (device-mode). Роблю РУЧНИЙ failover: вимкну мертвий канал.")
    # ручний тест кожного WAN і вимкнення мертвого
    def test_wan(gwip):
        talk(a,["/ip/route/add","=dst-address=9.9.9.9/32","=gateway="+gwip,"=comment=TESTX"])
        g=[at for r,at in a.talk(["/ping","=address=9.9.9.9","=count=3"]) if r=='!re']
        rec=int(g[-1].get('=received','0')) if g else 0
        for r in rows(a,"/ip/route/print"):
            if r.get('=comment')=='TESTX': a.talk(["/ip/route/remove","=.id="+r['=.id']])
        return rec
    if gw1 and test_wan(gw1)==0:
        for r in rows(a,"/ip/route/print"):
            if r.get('=comment','').startswith('LB-w1') and r.get('=dst-address')=='0.0.0.0/0':
                a.talk(["/ip/route/set","=.id="+r['=.id'],"=disabled=yes"])
        print("  WAN1 (ZTE) мертвий -> вимкнено, все через WAN2")
    if gw2 and test_wan(gw2)==0:
        for r in rows(a,"/ip/route/print"):
            if r.get('=comment','').startswith('LB-w2') and r.get('=dst-address')=='0.0.0.0/0':
                a.talk(["/ip/route/set","=.id="+r['=.id'],"=disabled=yes"])
        print("  WAN2 (Soyea) мертвий -> вимкнено, все через WAN1")

# звіт
print("\n-- netwatch стан --")
for r in rows(a,"/tool/netwatch/print"):
    if r.get('=comment','').startswith('LBnw'):
        print(f"   {r.get('=host')}: {r.get('=status')}")
print("-- активні default-маршрути --")
for r in rows(a,"/ip/route/print"):
    if r.get('=comment','').startswith('LB-w') and r.get('=dst-address')=='0.0.0.0/0':
        print(f"   {r.get('=comment'):9} table={r.get('=routing-table','main'):8} active={r.get('=active')} disabled={r.get('=disabled')}")
def rp(x):
    g=[at for r,at in a.talk(["/ping","=address="+x,"=count=2"]) if r=='!re']
    return g[-1].get('=received','0') if g else '0'
print(f"\nРоутер -> {PROBE1} (WAN1): {rp(PROBE1)}/2   -> {PROBE2} (WAN2): {rp(PROBE2)}/2")
s.close()
print("\n✅ Готово. Інтернет на UneasyM має працювати стабільно (мертвий канал авто-вимикається).")
print("   Скопіюй цей вивід Claude.")
