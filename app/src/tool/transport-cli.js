// CLI transport: runs a catalog tool by shelling out to the `wan-cli` Rust
// binary (built from app/src-tauri, sharing all RouterOS logic with the
// Tauri app's own commands in lib.rs) — used by the Node MCP/CLI entrypoint
// (app/bin/wan-monitor.mjs), which runs as a separate OS process and can't
// call Tauri's invoke().
import { spawnSync } from "node:child_process";
import { existsSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const TARGET_DIR = path.join(__dirname, "..", "..", "src-tauri", "target");

function cliBinaryPath() {
  const release = path.join(TARGET_DIR, "release", "wan_cli");
  if (existsSync(release)) return release;
  const debug = path.join(TARGET_DIR, "debug", "wan_cli");
  if (existsSync(debug)) return debug;
  throw new Error(
    `wan_cli binary not found (checked ${release} and ${debug}) — run "cargo build --bin wan_cli" in app/src-tauri`,
  );
}

export function cliTransport(tool, input) {
  const args = [tool.tauri];
  if (input && Object.keys(input).length) args.push(JSON.stringify(input));
  const res = spawnSync(cliBinaryPath(), args, { encoding: "utf8" });
  if (res.status !== 0) throw new Error((res.stderr || "").trim() || `${tool.tauri} exited ${res.status}`);
  return res.stdout;
}
