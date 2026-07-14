// The viewer has read-only tools only. RouterOS owns all failover decisions.

export const TOOLS = [
  {
    tier: "read",
    name: "read_router_events",
    summary:
      "Read recent MikroTik router log: netwatch status per channel and a flap-event history (channel disabled/re-enabled by netwatch, the sole failover mechanism — runs entirely on the router).",
    input: {},
    tauri: "read_router_log",
  },
  {
    tier: "read",
    name: "read_wan_speed",
    summary: "Read the current instantaneous rx/tx bits-per-second for both WAN interfaces.",
    input: {},
    tauri: "read_wan_speed",
  },
];
