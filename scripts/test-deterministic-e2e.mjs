#!/usr/bin/env node

/**
 * Run the deterministic end-to-end gates that can execute without a real
 * desktop window, external login, network, or a production Agent.
 *
 * This is an orchestration test, not a replacement for the manual Windows
 * Tauri/HWND acceptance checklist. Each child test still crosses its real
 * boundary: UI -> Tauri command mocks, ACP -> stdio, MCP -> stdio tools, and
 * repository -> SQLite.
 */

import { spawnSync } from "node:child_process";
import { existsSync } from "node:fs";
import process from "node:process";
import path from "node:path";
import { fileURLToPath } from "node:url";

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, "..");
const desktopRoot = path.join(repoRoot, "apps", "desktop");

const phases = [
  {
    name: "fixture-and-opt-in-script-contracts",
    command: "node",
    args: [
      "--test",
      "scripts/chapter-agent-e2e-fixture.test.mjs",
      "scripts/test-acp-mcp-tool-priority.test.mjs",
    ],
    cwd: repoRoot,
  },
  {
    name: "ui-chapter-and-chat-flow",
    command: "bun",
    args: [
      "run",
      "test",
      "--",
      "--project",
      "ui",
      "src/features/chapters/components/ChaptersPanel.e2e.test.tsx",
      "src/features/acp/components/conversationPresentation.integration.test.tsx",
    ],
    cwd: desktopRoot,
  },
  {
    name: "mcp-stdio-tool-and-database-boundary",
    command: "cargo",
    args: [
      "test",
      "-p",
      "lumina-mcp",
      "--features",
      "test-support",
      "--test",
      "chapter_blackbox_stdio",
      "--",
      "--nocapture",
    ],
    cwd: repoRoot,
    env: { LUMINA_CHAPTER_BLACKBOX_DELAY_MS: "1000" },
  },
  {
    name: "acp-stdio-session-boundary",
    command: "cargo",
    args: [
      "test",
      "-p",
      "lumina-acp",
      "--features",
      "test-support",
      "--test",
      "chapter_session_blackbox",
      "--",
      "--nocapture",
    ],
    cwd: repoRoot,
  },
  {
    name: "sqlite-crud-and-projection-boundary",
    command: "cargo",
    args: [
      "test",
      "-p",
      "lumina-library",
      "--test",
      "database_integration",
      "--test",
      "database_crud_integration",
      "--",
      "--nocapture",
    ],
    cwd: repoRoot,
  },
];

function runPhase(phase) {
  const startedAt = performance.now();
  const command = resolveCommand(phase.command);
  const result = spawnSync(command, phase.args, {
    cwd: phase.cwd,
    env: { ...process.env, ...phase.env },
    encoding: "utf8",
    windowsHide: true,
    stdio: ["ignore", "pipe", "pipe"],
  });
  const elapsedMs = Math.round(performance.now() - startedAt);
  const status = result.error
    ? "not_started"
    : result.status === 0
      ? "passed"
      : "failed";

  // Do not forward child output: ACP/MCP diagnostics may contain paths,
  // prompts, or Agent stderr. The individual command remains reproducible
  // from the phase name when a failure needs deeper inspection.
  return {
    name: phase.name,
    status,
    elapsedMs,
    exitCode: result.status,
  };
}

function resolveCommand(command) {
  if (process.platform !== "win32" || command !== "bun") return command;
  const bunHome = process.env.USERPROFILE;
  const bundledBun = bunHome
    ? path.join(bunHome, ".bun", "bin", "bun.exe")
    : null;
  return bundledBun && existsSync(bundledBun) ? bundledBun : command;
}

const startedAt = performance.now();
const results = [];
for (const phase of phases) {
  const result = runPhase(phase);
  results.push(result);
  console.log(
    `[e2e] phase=${result.name} status=${result.status} elapsed_ms=${result.elapsedMs}`,
  );
  if (result.status !== "passed") break;
}

const failed = results.find((result) => result.status !== "passed");
const report = {
  suite: "lumina-deterministic-e2e",
  status: failed ? "failed" : "passed",
  elapsedMs: Math.round(performance.now() - startedAt),
  phases: results,
  manualAcceptanceRequired: [
    "visible Windows Tauri window",
    "libmpv HWND sibling layout",
    "real Agent login and ACP/MCP session",
    "restart recovery with real application data",
  ],
};

console.log(JSON.stringify(report, null, 2));
process.exitCode = failed ? 1 : 0;
