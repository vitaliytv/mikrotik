// The viewer has read-only tools only. RouterOS owns all failover decisions.

export const TOOLS = [
  {
    tier: "read",
    name: "read_router_diagnostic",
    summary:
      "Check RouterOS API reachability, endpoint, response latency, scheduler health, script validity, and current primary WAN.",
    input: {},
    tauri: "read_router_diagnostic",
  },
  {
    tier: "read",
    name: "read_router_events",
    summary:
      "Read current RouterOS dual-WAN scheduler state, DHCP route priorities, pinned LMT/BITE health probes, and recent primary-WAN switch events.",
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
