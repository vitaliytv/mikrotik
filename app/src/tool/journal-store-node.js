// Plain-Node journal store for the MCP/CLI entrypoint — audit trail of agent
// requests, no Rust binary needed (unlike nitra/task, which spawns a compiled
// journal binary; mikrotik's journal is a low-concurrency audit trail, not core
// app state — see the plan's rationale). Writes to the same literal directory
// tauri-plugin-agent's journal_* commands use by default for this app's bundle
// id (com.vitalii.wan-monitor-app), unless AGENT_REQUESTS_DIR overrides it —
// both sides are deliberately not trying to resolve Tauri's path API exactly,
// just pointing at the same hardcoded path (macOS-only, single user).
import { mkdir, readFile, readdir, writeFile } from "node:fs/promises";
import { randomUUID } from "node:crypto";
import os from "node:os";
import path from "node:path";

function requestsDir() {
  return (
    process.env.AGENT_REQUESTS_DIR ||
    path.join(os.homedir(), "Library", "Application Support", "com.vitalii.wan-monitor-app", "requests")
  );
}

function recordPath(dir, id) {
  return path.join(dir, `${id}.json`);
}

export function createNodeJournalStore() {
  const dir = requestsDir();
  const ensureDir = () => mkdir(dir, { recursive: true });

  async function load(id) {
    const raw = await readFile(recordPath(dir, id), "utf8");
    return JSON.parse(raw);
  }

  return {
    async create({ intent, actor }) {
      await ensureDir();
      const id = randomUUID();
      const now = new Date().toISOString();
      const record = {
        id,
        createdAt: now,
        updatedAt: now,
        actor,
        intent,
        status: "pending",
        messages: [],
        actions: [],
        summary: null,
        question: null,
        error: null,
        pendingApproval: null,
        parentId: null,
      };
      await writeFile(recordPath(dir, id), JSON.stringify(record, null, 2));
      return id;
    },
    load,
    async update(id, patch) {
      const record = await load(id);
      const next = { ...record, ...patch, updatedAt: new Date().toISOString() };
      await writeFile(recordPath(dir, id), JSON.stringify(next, null, 2));
    },
    async list() {
      await ensureDir();
      const files = await readdir(dir);
      const records = [];
      for (const file of files) {
        if (!file.endsWith(".json")) continue;
        try {
          records.push(JSON.parse(await readFile(path.join(dir, file), "utf8")));
        } catch {
          // skip an unreadable/partial record
        }
      }
      return records;
    },
  };
}
