#!/usr/bin/env python3
"""
Manually enable/disable a WAN channel's routes (bypasses auto-failover for one run).
Usage: wan_toggle.py <zte|soyea> <on|off>  (zte=LMT/WAN1, soyea=BITE/WAN2)
Updates ~/wan_state.json so wan_monitor.py's cron doesn't fight the manual change
on its next run.
"""
import socket
import sys
import os

sys.path.insert(0, os.path.expanduser("~"))
import wan_monitor as wm  # noqa: E402


def main():
    if len(sys.argv) != 3 or sys.argv[1] not in ("zte", "soyea") or sys.argv[2] not in ("on", "off"):
        print("Usage: wan_toggle.py <zte|soyea> <on|off>")
        raise SystemExit(1)

    chan = sys.argv[1]
    enable = sys.argv[2] == "on"
    prefix = "1" if chan == "zte" else "2"

    sk = socket.socket()
    sk.settimeout(30)
    sk.connect((wm.HOST, 8728))
    a = wm.ApiRos(sk)
    if not a.login(wm.USER, wm.PW):
        print("login failed")
        raise SystemExit(1)

    n = wm.set_wan_routes(a, prefix, enable)
    sk.close()

    state = wm.load_state()
    state[chan]["disabled"] = not enable
    state[chan]["bad_streak"] = 0
    state[chan]["good_streak"] = 0
    wm.save_state(state)

    action = "увімкнено" if enable else "вимкнено вручну"
    print(f"{chan}: {action} ({n} маршрутів)")


if __name__ == "__main__":
    main()
