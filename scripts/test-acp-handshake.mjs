import { spawn } from "node:child_process";
import readline from "node:readline";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.dirname(path.dirname(fileURLToPath(import.meta.url)));
const codexPath =
  process.env.CODEX_PATH ??
  "C:\\Users\\zheye\\AppData\\Local\\Programs\\OpenAI\\Codex\\bin\\codex.exe";

const env = {
  ...process.env,
  CODEX_PATH: codexPath,
  CODEX_HOME: process.env.CODEX_HOME ?? path.join(process.env.USERPROFILE, ".codex"),
  TERM: "xterm-256color",
};

const entry = path.join(
  root,
  "node_modules",
  "@agentclientprotocol",
  "codex-acp",
  "dist",
  "index.js",
);

function send(child, payload) {
  child.stdin.write(`${JSON.stringify(payload)}\n`);
}

function waitLine(rl, id) {
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error(`timeout waiting for id=${id}`)), 60_000);
    const onLine = (line) => {
      if (!line.trim()) return;
      const value = JSON.parse(line);
      if (value.id === id) {
        clearTimeout(timer);
        rl.off("line", onLine);
        resolve(value);
      }
    };
    rl.on("line", onLine);
  });
}

const child = spawn(process.execPath, [entry], {
  cwd: root,
  env,
  stdio: ["pipe", "pipe", "pipe"],
  windowsHide: true,
});

child.stderr.on("data", (chunk) => process.stderr.write(chunk));

const rl = readline.createInterface({ input: child.stdout });

try {
  send(child, {
    jsonrpc: "2.0",
    id: 1,
    method: "initialize",
    params: {
      protocolVersion: 1,
      clientCapabilities: {
        fs: { readTextFile: true, writeTextFile: true },
        terminal: true,
      },
      clientInfo: { name: "lumina-test", title: "Lumina", version: "0.1.0" },
    },
  });
  const initResp = await waitLine(rl, 1);
  console.error("[ok] initialize", initResp.result?.agentInfo?.name);

  send(child, {
    jsonrpc: "2.0",
    id: 2,
    method: "authenticate",
    params: { methodId: "chat-gpt" },
  });
  const authResp = await waitLine(rl, 2);
  if (authResp.error) throw new Error(JSON.stringify(authResp.error));
  console.error("[ok] authenticate chat-gpt");

  const cwd = root.replace(/\//g, "\\");
  send(child, {
    jsonrpc: "2.0",
    id: 3,
    method: "session/new",
    params: { cwd, mcpServers: [] },
  });
  const sessionResp = await waitLine(rl, 3);
  if (sessionResp.error) throw new Error(JSON.stringify(sessionResp.error));
  console.error("[ok] session/new", sessionResp.result?.sessionId);
  child.kill();
  process.exit(0);
} catch (error) {
  console.error("[fail]", error);
  child.kill();
  process.exit(1);
}
