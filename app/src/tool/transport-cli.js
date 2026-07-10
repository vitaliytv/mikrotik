// CLI transport: runs a catalog tool by shelling out to the same Python
// scripts the Tauri commands wrap (see app/src-tauri/src/lib.rs) — used by the
// Node MCP/CLI entrypoint (app/bin/wan-monitor.mjs), which runs as a separate
// OS process and can't call Tauri's invoke(). Simpler than compiling a
// standalone Rust binary: the "backend" here already is a set of standalone
// Python scripts callable from any process.
import { spawnSync } from "node:child_process";
import { readFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";

function homePath(name) {
  return path.join(os.homedir(), name);
}

function runPython(script, args = []) {
  const res = spawnSync("/usr/bin/python3", [homePath(script), ...args], { encoding: "utf8" });
  const text = (res.stdout || "") + (res.stderr || "");
  if (res.status !== 0) throw new Error(text.trim() || `${script} exited ${res.status}`);
  return text;
}

const RUNNERS = {
  read_wan_csv: () => readFileSync(homePath("wan_log.csv"), "utf8"),
  read_wan_state: () => readFileSync(homePath("wan_state.json"), "utf8"),
  run_wan_monitor: () => runPython("wan_monitor.py"),
  read_router_log: () => runPython("wan_router_log.py"),
  toggle_wan: (input) => runPython("wan_toggle.py", [input.channel, input.on ? "on" : "off"]),
  restore_failover_config: () => runPython("fix_mikrotik.py"),
};

export function cliTransport(tool, input) {
  const runner = RUNNERS[tool.tauri];
  if (!runner) throw new Error(`No CLI runner for tool "${tool.name}"`);
  return runner(input ?? {});
}
