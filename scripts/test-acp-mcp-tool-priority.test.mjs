import test from "node:test";
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

const scriptsDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptsDir, "..");
const scriptPath = path.join(scriptsDir, "test-acp-mcp-tool-priority.mjs");

function runCli(args = [], overrides = {}, unset = []) {
  const env = { ...process.env, ...overrides };
  for (const name of unset) delete env[name];

  const result = spawnSync(process.execPath, [scriptPath, ...args], {
    cwd: repoRoot,
    env,
    encoding: "utf8",
    windowsHide: true,
  });
  assert.equal(result.error, undefined, result.error?.message);
  return result;
}

test("ACP/MCP priority CLI --help is deterministic", () => {
  const result = runCli(["--help"], {}, ["LUMINA_MCP_E2E"]);

  assert.equal(result.status, 0);
  assert.equal(result.stderr, "");
  assert.match(result.stdout, /Real ACP\/MCP priority E2E \(opt-in\)/);
  assert.match(result.stdout, /LUMINA_MCP_E2E=1/);
  assert.match(result.stdout, /LUMINA_MCP_E2E_TIMEOUT_MS/);
});

test("ACP/MCP priority CLI safely skips when opt-in is absent", () => {
  const result = runCli(
    [],
    {},
    ["LUMINA_MCP_E2E", "LUMINA_MCP_E2E_TIMEOUT_MS", "CODEX_PATH", "LUMINA_MCP_COMMAND"],
  );

  assert.equal(result.status, 0);
  assert.equal(result.stdout, "");
  assert.equal(
    result.stderr,
    "[skip] set LUMINA_MCP_E2E=1 to run the real Codex ACP/MCP priority check\n",
  );
});

test("ACP/MCP priority CLI rejects invalid timeouts before local dependency checks", () => {
  for (const timeout of ["not-a-number", "999"]) {
    const result = runCli(
      [],
      { LUMINA_MCP_E2E: "1", LUMINA_MCP_E2E_TIMEOUT_MS: timeout },
      ["CODEX_PATH", "LUMINA_MCP_COMMAND"],
    );

    assert.equal(result.status, 1, `timeout=${timeout}`);
    assert.equal(result.stdout, "", `timeout=${timeout}`);
    assert.equal(
      result.stderr,
      "[fail] invalid timeout: set LUMINA_MCP_E2E_TIMEOUT_MS to an integer >= 1000\n",
      `timeout=${timeout}`,
    );
  }
});
