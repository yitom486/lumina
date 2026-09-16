import { spawn } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import readline from "node:readline";
import { fileURLToPath } from "node:url";

const root = path.dirname(path.dirname(fileURLToPath(import.meta.url)));
const timeoutMs = Number.parseInt(process.env.LUMINA_MCP_E2E_TIMEOUT_MS ?? "90000", 10);
const codexPath =
  process.env.CODEX_PATH ??
  "C:\\Users\\zheye\\AppData\\Local\\Programs\\OpenAI\\Codex\\bin\\codex.exe";
const mcpCommand = process.env.LUMINA_MCP_COMMAND ?? path.join(root, "target", "debug", "lumina-app.exe");
const codexAcpEntry = path.join(
  root,
  "node_modules",
  "@agentclientprotocol",
  "codex-acp",
  "dist",
  "index.js",
);

if (process.env.LUMINA_MCP_E2E !== "1") {
  console.error("[skip] set LUMINA_MCP_E2E=1 to run the real Codex ACP/MCP priority check");
  process.exit(0);
}

for (const required of [codexPath, codexAcpEntry, mcpCommand]) {
  if (!fs.existsSync(required)) {
    console.error(`[skip] required local executable is unavailable: ${path.basename(required)}`);
    process.exit(0);
  }
}

function fail(message) {
  throw new Error(message);
}

function send(child, payload) {
  child.stdin.write(`${JSON.stringify(payload)}\n`);
}

function stop(child) {
  if (child && !child.killed) child.kill();
}

class RpcLines {
  constructor(stream) {
    this.responses = new Map();
    this.waiters = new Map();
    this.notifications = [];
    this.listeners = new Set();
    this.readline = readline.createInterface({ input: stream });
    this.readline.on("line", (line) => this.onLine(line));
  }

  onLine(line) {
    if (!line.trim()) return;
    let message;
    try {
      message = JSON.parse(line);
    } catch {
      return;
    }
    if (message.id !== undefined && message.id !== null) {
      const waiter = this.waiters.get(message.id);
      if (waiter) {
        this.waiters.delete(message.id);
        waiter.resolve(message);
      } else {
        this.responses.set(message.id, message);
      }
      return;
    }
    this.notifications.push(message);
    for (const listener of this.listeners) listener(message);
  }

  waitFor(id) {
    const existing = this.responses.get(id);
    if (existing) {
      this.responses.delete(id);
      return Promise.resolve(existing);
    }
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        this.waiters.delete(id);
        reject(new Error(`timeout waiting for RPC response ${id}`));
      }, timeoutMs);
      this.waiters.set(id, {
        resolve: (value) => {
          clearTimeout(timer);
          resolve(value);
        },
      });
    });
  }

  subscribe(listener) {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  close() {
    this.readline.close();
  }
}

function sessionEnv(snapshotPath) {
  return {
    ...process.env,
    LUMINA_MCP_CONTEXT_FILE: snapshotPath,
    LUMINA_MCP_TOOL_PROFILE: "chat",
  };
}

function mcpServer(snapshotPath) {
  return {
    name: "lumina",
    command: mcpCommand,
    args: ["--lumina-mcp"],
    env: [
      { name: "LUMINA_MCP_CONTEXT_FILE", value: snapshotPath },
      { name: "LUMINA_MCP_TOOL_PROFILE", value: "chat" },
    ],
  };
}

function snapshotFixture() {
  return {
    schemaVersion: 5,
    anchor: {
      mediaPath: path.join(os.tmpdir(), "lumina-mcp-e2e-fixture.mp4"),
      mediaTitle: "Lumina MCP priority fixture",
      libraryRoot: null,
      groupKey: null,
      season: 1,
      episode: 2,
      positionMs: 60000,
      durationMs: 120000,
      sentAtMs: Date.now(),
      subtitleChoiceId: null,
    },
    currentEpisode: {
      season: 1,
      episode: 2,
      title: "MCP priority fixture",
      overview: "用于验证 Agent 会先读取 Lumina 上下文，而不是搜索网络。",
    },
    library: null,
    session: { turn: 1, mediaPath: "fixture", libraryWarmedTurn: 1, libraryWarmEvery: 5 },
    capabilities: {
      visionCapable: false,
      subtitleWorkshopEnabled: false,
      videoAnnotationsEnabled: true,
    },
    online: null,
    updatedAtMs: Date.now(),
  };
}

async function verifyMcpCatalog(snapshotPath) {
  const child = spawn(mcpCommand, ["--lumina-mcp"], {
    cwd: root,
    env: sessionEnv(snapshotPath),
    stdio: ["pipe", "pipe", "pipe"],
    windowsHide: true,
  });
  const rpc = new RpcLines(child.stdout);
  try {
    send(child, { jsonrpc: "2.0", id: 1, method: "initialize", params: {} });
    const initialized = await rpc.waitFor(1);
    if (initialized.error) fail("Lumina MCP initialize failed");
    const instructions = initialized.result?.instructions ?? "";
    if (!instructions.includes("lumina_get_library_context")) {
      fail("Lumina MCP instructions did not expose the context-tool catalog");
    }

    send(child, { jsonrpc: "2.0", id: 2, method: "tools/list", params: {} });
    const listed = await rpc.waitFor(2);
    if (listed.error) fail("Lumina MCP tools/list failed");
    const names = (listed.result?.tools ?? []).map((tool) => tool.name);
    for (const required of ["lumina_get_library_context", "lumina_get_transcript_window"]) {
      if (!names.includes(required)) fail(`Lumina MCP tools/list omitted ${required}`);
    }
    console.error("[ok] Lumina MCP exposes the plot-context tools");
  } finally {
    rpc.close();
    stop(child);
  }
}

function stringsIn(value, output = []) {
  if (typeof value === "string") output.push(value);
  else if (Array.isArray(value)) value.forEach((item) => stringsIn(item, output));
  else if (value && typeof value === "object") Object.values(value).forEach((item) => stringsIn(item, output));
  return output;
}

function namesFromToolUpdate(message) {
  const update = message?.params?.update;
  if (!update || !["tool_call", "tool_call_update"].includes(update.sessionUpdate)) return [];
  return stringsIn(update).filter((value) => /^lumina_[a-z_]+$/.test(value) || /web[_-]?search/i.test(value));
}

async function verifyAgentPriority(snapshotPath, workspace) {
  const child = spawn(process.execPath, [codexAcpEntry], {
    cwd: root,
    env: {
      ...process.env,
      CODEX_PATH: codexPath,
      CODEX_HOME: process.env.CODEX_HOME ?? path.join(process.env.USERPROFILE, ".codex"),
      TERM: "xterm-256color",
    },
    stdio: ["pipe", "pipe", "pipe"],
    windowsHide: true,
  });
  child.stderr.on("data", () => {}); // Agent stderr is intentionally not surfaced by this test.
  const rpc = new RpcLines(child.stdout);
  const calls = [];
  const unsubscribe = rpc.subscribe((message) => calls.push(...namesFromToolUpdate(message)));
  try {
    send(child, {
      jsonrpc: "2.0",
      id: 1,
      method: "initialize",
      params: {
        protocolVersion: 1,
        clientCapabilities: { fs: { readTextFile: true, writeTextFile: true }, terminal: true },
        clientInfo: { name: "lumina-mcp-e2e", title: "Lumina MCP E2E", version: "0.3.0" },
      },
    });
    const init = await rpc.waitFor(1);
    if (init.error) fail("Codex ACP initialize failed");

    send(child, { jsonrpc: "2.0", id: 2, method: "authenticate", params: { methodId: "chat-gpt" } });
    const auth = await rpc.waitFor(2);
    if (auth.error) fail("Codex ACP authentication failed");

    send(child, {
      jsonrpc: "2.0",
      id: 3,
      method: "session/new",
      params: { cwd: workspace, mcpServers: [mcpServer(snapshotPath)] },
    });
    const created = await rpc.waitFor(3);
    if (created.error || !created.result?.sessionId) fail("Codex ACP session/new failed");

    send(child, {
      jsonrpc: "2.0",
      id: 4,
      method: "session/prompt",
      params: {
        sessionId: created.result.sessionId,
        prompt: [{ type: "text", text: "这一集主要讲了怎样的剧情？请仅依据 Lumina 当前媒体上下文回答。" }],
      },
    });
    const answered = await rpc.waitFor(4);
    if (answered.error) fail("Codex ACP prompt failed");

    if (calls.some((name) => /web[_-]?search/i.test(name))) {
      fail("Agent used web search before answering the episode-plot prompt");
    }
    const firstLumina = calls.find((name) => name.startsWith("lumina_"));
    if (!firstLumina) fail("Agent did not call a Lumina context tool");
    if (!new Set(["lumina_get_library_context", "lumina_get_transcript_window"]).has(firstLumina)) {
      fail(`first Lumina tool for plot was ${firstLumina}, not a plot-context tool`);
    }
    console.error(`[ok] plot prompt used ${firstLumina} before any web search`);
  } finally {
    unsubscribe();
    rpc.close();
    stop(child);
  }
}

const workspace = fs.mkdtempSync(path.join(os.tmpdir(), "lumina-mcp-priority-"));
const snapshotPath = path.join(workspace, ".lumina", "agent-context.json");
fs.mkdirSync(path.dirname(snapshotPath), { recursive: true });
fs.writeFileSync(snapshotPath, JSON.stringify(snapshotFixture()));

try {
  await verifyMcpCatalog(snapshotPath);
  await verifyAgentPriority(snapshotPath, workspace);
} catch (error) {
  console.error(`[fail] ${error instanceof Error ? error.message : "unknown failure"}`);
  process.exitCode = 1;
} finally {
  fs.rmSync(workspace, { recursive: true, force: true });
}
