#!/bin/zsh
set -euo pipefail

ROOT="${0:A:h:h}"
TAURI_DIR="$ROOT/app/src-tauri"
CLI="$TAURI_DIR/target/debug/wan_cli"

if [[ ! -x "$CLI" ]]; then
  print "Building local diagnostic binary..."
  (cd "$TAURI_DIR" && cargo build --bin wan_cli)
fi

print "Observing RouterOS every 5 seconds. Press Ctrl-C to stop."
while true; do
  print "\n===== $(date '+%Y-%m-%d %H:%M:%S') ====="
  "$CLI" read_router_log | jq '{
    controller,
    dhcp,
    routes,
    probes,
    switch_events,
    quality_events,
    dualwan_log: [.raw_log[] | select(.message | startswith("DUALWAN"))]
  }'
  sleep 5
done
