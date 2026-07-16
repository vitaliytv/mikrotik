#!/usr/bin/env node
// Headless entrypoint for the WAN-monitor agent gateway — CLI modes for
// scripting/debugging, plus (added separately) an `mcp` mode that registers a
// stdio MCP server so an external orchestrator (Claude Code) can delegate
// natural-language intents to the same agent the in-app chat panel drives.
import { Server } from "@modelcontextprotocol/sdk/server/index.js";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import { CallToolRequestSchema, ListToolsRequestSchema } from "@modelcontextprotocol/sdk/types.js";
import { createAgentKit, createOpenAiChat, listTools, toolManifest } from "@7n/tauri-components";
import { TOOLS } from "../src/tool/catalog.js";
import { createSystemPrompt } from "../src/tool/prompt.js";
import { cliTransport } from "../src/tool/transport-cli.js";
import { createNodeJournalStore } from "../src/tool/journal-store-node.js";
import { resolveOmlxConfig } from "../src/tool/omlx-config-node.js";

const journal = createNodeJournalStore();

const kit = createAgentKit({
  catalog: TOOLS,
  systemPrompt: createSystemPrompt(),
  transport: cliTransport,
  journal,
});

function parseArgs(argv) {
  const actorIdx = argv.indexOf("--actor");
  const positional = [...argv];
  let actorId;
  if (actorIdx !== -1) {
    actorId = argv[actorIdx + 1];
    positional.splice(actorIdx, 2);
  }
  return { actorId, positional };
}

function chatFromEnv() {
  const { baseUrl, model, apiKey } = resolveOmlxConfig();
  return createOpenAiChat({ baseUrl, model, apiKey });
}

function printUsage() {
  console.error(
    [
      "Usage:",
      "  wan-monitor.mjs list",
      "  wan-monitor.mjs schema",
      "  wan-monitor.mjs dispatch <tool> [json-input]",
      "  wan-monitor.mjs agent <prompt> [--actor id]",
      "  wan-monitor.mjs mcp [--actor id]",
    ].join("\n"),
  );
}

async function main() {
  const [, , cmd, ...rest] = process.argv;

  if (cmd === "list") {
    console.log(JSON.stringify(listTools(TOOLS), null, 2));
    return;
  }

  if (cmd === "schema") {
    console.log(JSON.stringify(toolManifest(TOOLS), null, 2));
    return;
  }

  if (cmd === "dispatch") {
    const [toolName, inputJson] = rest;
    const input = inputJson ? JSON.parse(inputJson) : {};
    const envelope = await kit.dispatch(toolName, input);
    console.log(JSON.stringify(envelope, null, 2));
    if (!envelope.ok) process.exitCode = 1;
    return;
  }

  if (cmd === "agent") {
    const { actorId, positional } = parseArgs(rest);
    const intent = positional.join(" ");
    if (!intent) {
      printUsage();
      process.exitCode = 1;
      return;
    }
    const actor = { kind: "agent", id: actorId || "cli" };
    const result = await kit.request({ intent, actor, chat: chatFromEnv() });
    console.log(JSON.stringify(result, null, 2));
    return;
  }

  if (cmd === "mcp") {
    await runMcpServer(parseArgs(rest).actorId);
    // No process.exit here: the stdio transport keeps stdin open as an active
    // handle, so the process stays alive serving requests and only exits on
    // stdin EOF/disconnect. Calling process.exit right after connect() would
    // kill the server before it ever served a request.
    return;
  }

  printUsage();
  process.exitCode = 1;
}

// Two tools only — `request(intent)` to start, `respond(requestId, message)`
// to resume a clarification — mirroring nitra/task's agent-gateway contract so
// an external orchestrator (Claude Code) never needs to know this app's tool
// names or CLI conventions, only these two.
async function runMcpServer(actorId) {
  const actor = { kind: "agent", id: actorId ?? process.env.MCP_ACTOR_ID ?? "mcp" };

  const server = new Server({ name: "wan-monitor", version: "1.0.0" }, { capabilities: { tools: {} } });

  server.setRequestHandler(ListToolsRequestSchema, () => ({
    tools: [
      {
        name: "request",
        description: "Start a new WAN-monitor agent request from a natural-language intent.",
        inputSchema: {
          type: "object",
          properties: { intent: { type: "string", description: "What to do, e.g. 'read current WAN state'." } },
          required: ["intent"],
        },
      },
      {
        name: "respond",
        description: "Continue a WAN-monitor agent request that needs clarification, with a follow-up message.",
        inputSchema: {
          type: "object",
          properties: {
            requestId: { type: "string", description: "The requestId returned by an earlier request/respond call." },
            message: { type: "string", description: "The follow-up answer." },
          },
          required: ["requestId", "message"],
        },
      },
    ],
  }));

  server.setRequestHandler(CallToolRequestSchema, async (req) => {
    const { name, arguments: args } = req.params;
    try {
      let result;
      if (name === "request") {
        result = await kit.request({ intent: args.intent, actor, chat: chatFromEnv() });
      } else if (name === "respond") {
        result = await kit.respond({ requestId: args.requestId, message: args.message, actor, chat: chatFromEnv() });
      } else {
        return { content: [{ type: "text", text: JSON.stringify({ error: `Unknown tool: ${name}` }) }], isError: true };
      }
      return { content: [{ type: "text", text: JSON.stringify(result, null, 2) }] };
    } catch (err) {
      return { content: [{ type: "text", text: JSON.stringify({ error: String(err?.message ?? err) }) }], isError: true };
    }
  });

  await server.connect(new StdioServerTransport());
}

main().catch((err) => {
  console.error(err?.stack ?? String(err));
  process.exitCode = 1;
});
