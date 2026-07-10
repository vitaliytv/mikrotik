// Tool catalog for the WAN-monitor agent gateway. Each entry follows the
// @7n/tauri-components catalog contract: { tier, name, summary, input, tauri, validate? }.
// tier 'read'/'write': the agent executes freely. tier 'destructive': the agent
// pauses for human approval (see @7n/tauri-components' scope.js).

function validateChannel(input) {
  if (input.channel !== "zte" && input.channel !== "soyea") {
    return 'channel must be "zte" or "soyea"';
  }
  return null;
}

export const TOOLS = [
  {
    tier: "read",
    name: "read_wan_log",
    summary:
      "Read the full ~/wan_log.csv measurement history (timestamp, RTT avg/max, loss%, active flag per WAN channel).",
    input: {},
    tauri: "read_wan_csv",
  },
  {
    tier: "read",
    name: "read_wan_state",
    summary:
      "Read the current per-WAN failover state (~/wan_state.json): disabled flag and bad/good streak counts for each channel.",
    input: {},
    tauri: "read_wan_state",
  },
  {
    tier: "read",
    name: "read_router_events",
    summary:
      "Read recent MikroTik router log: netwatch status per channel and a flap-event history (channel disabled/re-enabled).",
    input: {},
    tauri: "read_router_log",
  },
  {
    tier: "write",
    name: "measure_now",
    summary:
      "Trigger an immediate WAN quality measurement (ping both channels via the router) instead of waiting for the next cron run; may also disable/enable a channel per quality thresholds.",
    input: {},
    tauri: "run_wan_monitor",
  },
  {
    tier: "destructive",
    name: "toggle_wan",
    summary:
      "Manually enable or disable a WAN channel's default routes on the router, bypassing quality-based auto-failover. Destructive — requires human approval.",
    input: {
      channel: {
        type: "string",
        required: true,
        description: 'Which channel: "zte" (LMT / WAN1) or "soyea" (BITE / WAN2).',
      },
      on: {
        type: "boolean",
        required: true,
        description: "true to enable, false to disable.",
      },
    },
    validate: validateChannel,
    tauri: "toggle_wan",
  },
  {
    tier: "destructive",
    name: "restore_failover_config",
    summary:
      "Re-apply netwatch rules, routes and failover configuration on the router (manual recovery after config loss). Destructive — requires human approval.",
    input: {},
    tauri: "restore_failover_config",
  },
];
