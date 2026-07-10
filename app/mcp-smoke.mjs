#!/usr/bin/env node
// Smoke test for bin/wan-monitor.mjs's `mcp` mode: spawns it as a real
// subprocess and talks MCP-stdio to it, same as an external orchestrator
// (Claude Code) would via .mcp.json. Run: node mcp-smoke.mjs
import { Client } from "@modelcontextprotocol/sdk/client/index.js";
import { StdioClientTransport } from "@modelcontextprotocol/sdk/client/stdio.js";

const transport = new StdioClientTransport({
  command: "node",
  args: ["bin/wan-monitor.mjs", "mcp", "--actor", "smoke-test"],
});

const client = new Client({ name: "smoke-client", version: "1.0.0" }, { capabilities: {} });
await client.connect(transport);

const tools = await client.listTools();
const names = tools.tools.map((t) => t.name);
console.log("tools/list:", JSON.stringify(names));
if (JSON.stringify(names) !== JSON.stringify(["request", "respond"])) {
  throw new Error(`unexpected tool list: ${names}`);
}

const result = await client.callTool({ name: "request", arguments: { intent: "read current wan state" } });
const payload = JSON.parse(result.content[0].text);
console.log("tools/call request ->", JSON.stringify(payload, null, 2));
if (payload.status !== "done" && payload.status !== "failed") {
  throw new Error(`unexpected status: ${payload.status}`);
}

await client.close();
console.log("OK");
process.exit(0);
